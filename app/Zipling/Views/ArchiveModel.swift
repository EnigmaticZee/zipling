import AppKit
import Foundation
import Observation

/// A row in the browser: a file, link or folder inside the archive.
struct Item: Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let parent: String
    let isFolder: Bool
    let kind: String
    var size: UInt64
    var packed: UInt64
    /// False when the format doesn't record how small this item got.
    var packedKnown = true
    let modified: Date
    let transform: String
    let codec: String
    var encrypted = false
    var childCount = 0

    var saved: Double { size == 0 ? 0 : max(0, 1 - Double(packed) / Double(size)) }
    var isArchive: Bool { !isFolder && Archives.looksLikeArchive(name) }
}

/// A password the window is waiting for, and what to do with it.
struct PasswordRequest: Identifiable {
    let id = UUID()
    let archiveName: String
    let wrong: Bool
    let retry: @MainActor (String) -> Void
    let giveUp: @MainActor () -> Void
}

/// One open archive window: its contents, nested archives opened inside it,
/// navigation state and the running job.
@MainActor
@Observable
final class ArchiveModel {
    enum State {
        case loading
        case ready
        case failed(String)
    }

    /// One archive in the stack: the window's own, or one opened from inside it.
    struct Level {
        let name: String
        var archive: OpenedArchive
        let children: [String: [Item]]
        let everything: [Item]
        let byID: [Item.ID: Item]
        /// Where the browser was inside this archive, restored on the way back.
        var folder = ""
    }

    let url: URL
    private(set) var state: State = .loading
    private(set) var levels: [Level] = []
    /// Set while an archive inside this one is being opened.
    private(set) var openingNested: String?

    var folder = ""
    var search = ""
    var selection: Set<Item.ID> = []
    var sortOrder = [KeyPathComparator(\Item.name, comparator: .localizedStandard)]

    var job: Job?
    private(set) var jobOutcome: String?
    var passwordRequest: PasswordRequest?
    var alert: String?

    /// Private working folder: unwrapped tars, nested archives, files opened from the archive.
    @ObservationIgnored let scratch = FileManager.default.temporaryDirectory
        .appending(path: "Zipling/\(UUID().uuidString)")
    @ObservationIgnored private var quarantine: Data?

    init(url: URL) {
        self.url = url
    }

    var name: String { url.deletingPathExtension().lastPathComponent }
    var listing: Listing? { levels.last?.archive.listing }
    private var level: Level? { levels.last }

    func load(password: String? = nil) async {
        quarantine = SafeExtraction.quarantine(of: url)
        do {
            let archive = try await Archives.open(url, password: password, scratch: scratch)
            levels = [await Self.makeLevel(name: url.lastPathComponent, archive: archive)]
            state = .ready
        } catch let p as PasswordRequired {
            askPassword(for: url.lastPathComponent, wrong: p.wrong) { [weak self] password in
                Task { await self?.load(password: password) }
            } giveUp: { [weak self] in
                self?.state = .failed("This archive is encrypted. Its contents can be shown once you enter its password.")
            }
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    /// Removes the private working folder (called when the window closes).
    func close() {
        try? FileManager.default.removeItem(at: scratch)
    }

    private static func makeLevel(name: String, archive: OpenedArchive) async -> Level {
        let entries = archive.listing.entries
        return await Task.detached(priority: .userInitiated) {
            let (children, everything) = buildTree(entries)
            let byID = Dictionary(everything.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            return Level(name: name, archive: archive, children: children, everything: everything, byID: byID)
        }.value
    }

    private func askPassword(
        for name: String, wrong: Bool, retry: @escaping @MainActor (String) -> Void,
        giveUp: @escaping @MainActor () -> Void = {}
    ) {
        passwordRequest = PasswordRequest(archiveName: name, wrong: wrong, retry: retry, giveUp: giveUp)
    }

    // MARK: Navigation

    var visibleItems: [Item] {
        guard let level else { return [] }
        let base = search.isEmpty
            ? level.children[folder] ?? []
            : level.everything.filter { $0.name.localizedCaseInsensitiveContains(search) }
        let sorted = base.sorted(using: sortOrder)
        return sorted.filter(\.isFolder) + sorted.filter { !$0.isFolder }
    }

    /// Folders inside the current archive, for the path bar.
    var crumbs: [(name: String, path: String)] {
        guard !folder.isEmpty else { return [] }
        var out: [(String, String)] = []
        var path = ""
        for part in folder.split(separator: "/") {
            path = path.isEmpty ? String(part) : "\(path)/\(part)"
            out.append((String(part), path))
        }
        return out
    }

    /// Names of the archives opened one inside another, outermost first.
    var levelNames: [String] { levels.map(\.name) }

    func item(_ id: Item.ID) -> Item? {
        level?.byID[id]
    }

    var selectedItem: Item? {
        selection.count == 1 ? selection.first.flatMap(item) : nil
    }

    func enter(_ item: Item) {
        guard item.isFolder else { return }
        search = ""
        folder = item.id
        selection = []
    }

    func goUp() {
        if !search.isEmpty {
            search = ""
        } else if let slash = folder.lastIndex(of: "/") {
            folder = String(folder[..<slash])
        } else if !folder.isEmpty {
            folder = ""
        } else if levels.count > 1 {
            leaveTo(level: levels.count - 2)
            return
        }
        selection = []
    }

    /// Returns to an outer archive (0 is the window's own).
    func leaveTo(level index: Int) {
        guard index < levels.count - 1 else { return }
        levels.removeSubrange((index + 1)...)
        folder = levels[index].folder
        search = ""
        selection = []
    }

    var canGoUp: Bool { !folder.isEmpty || !search.isEmpty || levels.count > 1 }

    // MARK: Nested archives

    /// Opens an archive stored inside the current one, as a new level.
    func openNested(_ item: Item, password: String? = nil) {
        guard let level, openingNested == nil else { return }
        openingNested = item.name
        Task {
            defer { openingNested = nil }
            do {
                let inner = try await Archives.openNested(
                    item.id, in: level.archive, scratch: scratch, quarantine: quarantine, password: password)
                levels[levels.count - 1].folder = folder
                levels.append(await Self.makeLevel(name: item.name, archive: inner))
                folder = ""
                search = ""
                selection = []
            } catch let p as PasswordRequired {
                askPassword(for: item.name, wrong: p.wrong) { [weak self] password in
                    self?.openNested(item, password: password)
                }
            } catch {
                alert = "Couldn't open “\(item.name)”: \(error.localizedDescription)"
            }
        }
    }

    // MARK: Actions

    /// Double-click: open folders and archives inside, and open other files with their default app.
    func activate(_ ids: Set<Item.ID>) {
        guard let id = ids.first, let item = item(id) else { return }
        if item.isFolder {
            enter(item)
        } else if item.isArchive {
            openNested(item)
        } else {
            openWithDefaultApp(item)
        }
    }

    func openWithDefaultApp(_ item: Item) {
        let temp = scratch.appending(path: "open-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
        extract([item.id], to: temp, then: .open)
    }

    /// Asks where to, then extracts the given items (or everything).
    func extractInteractively(_ ids: Set<Item.ID>) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.canCreateDirectories = true
        panel.directoryURL = url.deletingLastPathComponent()
        panel.prompt = "Extract Here"
        panel.message = ids.isEmpty
            ? "Choose where to extract “\(level?.name ?? name)”"
            : "Choose where to extract \(ids.count == 1 ? "1 item" : "\(ids.count) items")"
        guard panel.runModal() == .OK, let dest = panel.url else { return }
        extract(ids, to: dest, then: .reveal)
    }

    enum AfterExtract { case reveal, open, nothing }

    func extract(_ ids: Set<Item.ID>, to dest: URL, then after: AfterExtract) {
        guard let level else { return }
        // Selecting a folder and something inside it means just the folder.
        let picked = ids.sorted()
        let roots = picked.filter { p in !picked.contains { p != $0 && p.hasPrefix($0 + "/") } }
        let job = Job(title: roots.isEmpty
            ? "Extracting “\(level.name)”…"
            : "Extracting \(roots.count == 1 ? (roots[0] as NSString).lastPathComponent : "\(roots.count) items")…")
        self.job = job
        jobOutcome = nil
        let archive = level.archive
        let quarantine = quarantine
        Task {
            // Extract into a hidden staging folder on the same volume, check and finish
            // it there, then move the results into place under Finder-style names:
            // nothing is ever overwritten, and a cancelled extraction leaves nothing behind.
            let staging = dest.appending(path: ".zipling-extract-\(UUID().uuidString)")
            do {
                let result = try await Archives.extract(
                    archive, to: staging, only: roots, task: job.task, cancel: job.cancelToken,
                    progress: job.progressHandler)
                let report = try SafeExtraction.finalize(staging, quarantine: quarantine)
                var placed: [URL] = []
                // Everything: whatever actually landed in staging. A selection: those items.
                let items = roots.isEmpty ? SafeExtraction.names(in: staging.path).sorted() : roots
                for path in items {
                    let source = staging.appending(path: path)
                    guard FileManager.default.fileExists(atPath: source.path) else { continue }
                    let target = Placement.unique((path as NSString).lastPathComponent, in: dest)
                    try FileManager.default.moveItem(at: source, to: target)
                    placed.append(target)
                }
                try? FileManager.default.removeItem(at: staging)
                var outcome = "Extracted \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) in \(Format.duration(result.seconds))."
                if archive.backend == .arca { outcome += " Every byte was verified." }
                if let skipped = result.skipped, !skipped.isEmpty {
                    outcome += " \(Format.count(UInt64(skipped.count), "item")) "
                        + (skipped.count == 1 ? "was" : "were") + " left out because writing "
                        + (skipped.count == 1 ? "it" : "them") + " would have been unsafe."
                }
                if !report.removedLinks.isEmpty {
                    outcome += " \(Format.count(UInt64(report.removedLinks.count), "link")) pointing outside the archive "
                        + (report.removedLinks.count == 1 ? "was" : "were") + " left out for safety."
                }
                jobOutcome = outcome
                job.finish(nil)
                switch after {
                case .reveal:
                    NSWorkspace.shared.activateFileViewerSelecting(placed)
                case .open:
                    if let first = placed.first { NSWorkspace.shared.open(first) }
                    self.job = nil
                case .nothing:
                    break
                }
            } catch let p as PasswordRequired {
                try? FileManager.default.removeItem(at: staging)
                self.job = nil
                askPassword(for: level.name, wrong: p.wrong) { [weak self] password in
                    self?.setPassword(password)
                    self?.extract(ids, to: dest, then: after)
                }
            } catch {
                try? FileManager.default.removeItem(at: staging)
                job.finish(error)
            }
        }
    }

    func verify() {
        guard let level else { return }
        let job = Job(title: "Verifying “\(level.name)”…")
        self.job = job
        jobOutcome = nil
        let archive = level.archive
        Task {
            do {
                let result = try await Archives.verify(archive, task: job.task, cancel: job.cancelToken,
                                                       progress: job.progressHandler)
                jobOutcome = archive.backend == .arca
                    ? "All \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) decoded and matched their BLAKE3 hashes in \(Format.duration(result.seconds))."
                    : "All \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) decoded and passed the archive's checks in \(Format.duration(result.seconds))."
                job.finish(nil)
            } catch let p as PasswordRequired {
                self.job = nil
                askPassword(for: level.name, wrong: p.wrong) { [weak self] password in
                    self?.setPassword(password)
                    self?.verify()
                }
            } catch {
                job.finish(error)
            }
        }
    }

    private func setPassword(_ password: String) {
        guard !levels.isEmpty else { return }
        levels[levels.count - 1].archive.password = password
    }

    func dismissJob() {
        job = nil
        jobOutcome = nil
    }

    // MARK: Tree

    /// Groups entries by parent folder and totals folder sizes. Folders that the
    /// archive doesn't list explicitly are synthesised.
    nonisolated static func buildTree(_ entries: [ArchiveEntry]) -> ([String: [Item]], [Item]) {
        var folders: [String: Item] = [:]
        var files: [Item] = []

        func parentOf(_ path: String) -> String {
            guard let slash = path.lastIndex(of: "/") else { return "" }
            return String(path[..<slash])
        }
        func folder(_ path: String, mtime: Int64 = 0) {
            if folders[path] == nil {
                folders[path] = Item(
                    id: path, name: (path as NSString).lastPathComponent, parent: parentOf(path),
                    isFolder: true, kind: "dir", size: 0, packed: 0,
                    modified: Date(timeIntervalSince1970: TimeInterval(mtime)), transform: "none", codec: "store")
            }
        }

        for e in entries {
            if e.isDirectory {
                folder(e.path, mtime: e.mtime)
                continue
            }
            let packedKnown = e.packedKnown ?? true
            files.append(Item(
                id: e.path, name: e.name, parent: e.parent, isFolder: false, kind: e.kind,
                size: e.size, packed: e.packed, packedKnown: packedKnown,
                modified: Date(timeIntervalSince1970: TimeInterval(e.mtime)),
                transform: e.transform, codec: e.codec, encrypted: e.encrypted ?? false))
            var p = e.parent
            while !p.isEmpty {
                folder(p)
                folders[p]!.size += e.size
                folders[p]!.packed += e.packed
                if !packedKnown { folders[p]!.packedKnown = false }
                p = parentOf(p)
            }
        }
        // Explicit folders may also be missing ancestors.
        for path in Array(folders.keys) {
            var p = parentOf(path)
            while !p.isEmpty {
                folder(p)
                p = parentOf(p)
            }
        }

        var children: [String: [Item]] = [:]
        for f in files { children[f.parent, default: []].append(f) }
        for f in folders.values { children[f.parent, default: []].append(f) }
        for (path, var f) in folders {
            f.childCount = children[path]?.count ?? 0
            folders[path] = f
        }
        // Re-insert folders with their final child counts.
        for (parent, items) in children {
            children[parent] = items.map { $0.isFolder ? folders[$0.id]! : $0 }
        }
        return (children, files + Array(folders.values))
    }
}
