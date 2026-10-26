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
    let modified: Date
    let transform: String
    let codec: String
    var childCount = 0

    var saved: Double { size == 0 ? 0 : max(0, 1 - Double(packed) / Double(size)) }
}

/// One open archive: its contents, navigation state and running job.
@MainActor
@Observable
final class ArchiveModel {
    enum State {
        case loading
        case ready
        case failed(String)
    }

    let url: URL
    private(set) var state: State = .loading
    private(set) var listing: Listing?
    private var children: [String: [Item]] = [:]
    private var everything: [Item] = []
    private var byID: [Item.ID: Item] = [:]

    var folder = ""
    var search = ""
    var selection: Set<Item.ID> = []
    var sortOrder = [KeyPathComparator(\Item.name, comparator: .localizedStandard)]

    var job: Job?
    private(set) var jobOutcome: String?

    init(url: URL) {
        self.url = url
    }

    var name: String { url.deletingPathExtension().lastPathComponent }

    func load() async {
        do {
            let listing = try await Engine.list(url)
            let (children, everything, byID) = await Task.detached(priority: .userInitiated) {
                let (children, everything) = Self.buildTree(listing.entries)
                return (children, everything, Dictionary(everything.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a }))
            }.value
            self.listing = listing
            self.children = children
            self.everything = everything
            self.byID = byID
            state = .ready
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    // MARK: Navigation

    var visibleItems: [Item] {
        let base = search.isEmpty
            ? children[folder] ?? []
            : everything.filter { $0.name.localizedCaseInsensitiveContains(search) }
        let sorted = base.sorted(using: sortOrder)
        return sorted.filter(\.isFolder) + sorted.filter { !$0.isFolder }
    }

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

    func item(_ id: Item.ID) -> Item? {
        byID[id]
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
        } else {
            folder = ""
        }
        selection = []
    }

    // MARK: Actions

    /// Double-click: open folders, and open files with their default app.
    func activate(_ ids: Set<Item.ID>) {
        guard let id = ids.first, let item = item(id) else { return }
        if item.isFolder {
            enter(item)
        } else {
            let temp = FileManager.default.temporaryDirectory.appending(path: "Arca/\(UUID().uuidString)")
            try? FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
            extract([id], to: temp, then: .open)
        }
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
            ? "Choose where to extract “\(name)”"
            : "Choose where to extract \(ids.count == 1 ? "1 item" : "\(ids.count) items")"
        guard panel.runModal() == .OK, let dest = panel.url else { return }
        extract(ids, to: dest, then: .reveal)
    }

    enum AfterExtract { case reveal, open, nothing }

    func extract(_ ids: Set<Item.ID>, to dest: URL, then after: AfterExtract) {
        // Selecting a folder and something inside it means just the folder.
        let picked = ids.sorted()
        let roots = picked.filter { p in !picked.contains { p != $0 && p.hasPrefix($0 + "/") } }
        let topLevel = (children[""] ?? []).map(\.id)
        let job = Job(title: roots.isEmpty ? "Extracting “\(name)”…" : "Extracting \(roots.count == 1 ? (roots[0] as NSString).lastPathComponent : "\(roots.count) items")…")
        self.job = job
        jobOutcome = nil
        let archive = url
        Task {
            // Extract into a hidden staging folder on the same volume, then move the
            // results into place under Finder-style names: nothing is ever
            // overwritten, and a cancelled extraction leaves nothing behind.
            let staging = dest.appending(path: ".arca-extract-\(UUID().uuidString)")
            do {
                let result = try await Engine.extract(archive, to: staging, only: roots, task: job.task, progress: job.progressHandler)
                var placed: [URL] = []
                for path in roots.isEmpty ? topLevel : roots {
                    let target = Placement.unique((path as NSString).lastPathComponent, in: dest)
                    try FileManager.default.moveItem(at: staging.appending(path: path), to: target)
                    placed.append(target)
                }
                try? FileManager.default.removeItem(at: staging)
                jobOutcome = "Extracted \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) in \(Format.duration(result.seconds)). Every byte was verified."
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
            } catch {
                try? FileManager.default.removeItem(at: staging)
                job.finish(error)
            }
        }
    }

    func verify() {
        let job = Job(title: "Verifying “\(name)”…")
        self.job = job
        jobOutcome = nil
        let archive = url
        Task {
            do {
                let result = try await Engine.verify(archive, task: job.task, progress: job.progressHandler)
                jobOutcome = "All \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) decoded and matched their BLAKE3 hashes in \(Format.duration(result.seconds))."
                job.finish(nil)
            } catch {
                job.finish(error)
            }
        }
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
            files.append(Item(
                id: e.path, name: e.name, parent: e.parent, isFolder: false, kind: e.kind,
                size: e.size, packed: e.packed, modified: Date(timeIntervalSince1970: TimeInterval(e.mtime)),
                transform: e.transform, codec: e.codec))
            var p = e.parent
            while !p.isEmpty {
                folder(p)
                folders[p]!.size += e.size
                folders[p]!.packed += e.packed
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
