import AppKit
import Foundation
import Observation
import CoreTransferable
import UniformTypeIdentifiers

/// A row in the browser: a file, link or folder inside the archive.
struct Item: Identifiable, Hashable, Sendable {
    /// Unique in the window: the entry's path, or for a search hit inside an archive
    /// within this one, that archive's ID and the entry's path.
    var id: String
    /// The entry's path inside its own archive.
    let path: String
    let name: String
    let parent: String
    let isFolder: Bool
    let kind: String
    var size: UInt64
    var packed: UInt64
    /// False when the format doesn't record how small this item got.
    var packedKnown = true
    var modified: Date
    let transform: String
    let codec: String
    var encrypted = false
    var childCount = 0
    /// For a search hit inside an archive within this one: that archive's item ID.
    var inArchive: String?

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
/// navigation, previews and running jobs.
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
        /// Each folder's subfolders, sorted, for the sidebar.
        let subfolders: [String: [String]]
        /// Where the browser was inside this archive, restored on the way back.
        var folder = ""
    }

    /// Files up to this size are pulled out for the preview pane without asking.
    static let autoPreviewLimit: UInt64 = 64 << 20
    /// Archives inside this one up to this size are opened when search looks inside them.
    static let searchInsideLimit: UInt64 = 2 << 30

    let url: URL
    private(set) var state: State = .loading
    private(set) var levels: [Level] = []
    /// Set while an archive inside this one is being opened.
    private(set) var openingNested: String?

    var folder = "" { didSet { if folder != oldValue { folderChanged() } } }
    var search = "" { didSet { if search != oldValue { refresh() } } }
    /// Search also looks inside the archives in this one.
    var searchInside = false { didSet { if searchInside != oldValue { refresh() } } }
    var selection: Set<Item.ID> = [] { didSet { if selection != oldValue { selectionChanged() } } }
    var sortOrder = [KeyPathComparator(\Item.name, comparator: .localizedStandard)] { didSet { refresh() } }
    /// What the list shows: the folder's contents, or search results, sorted.
    private(set) var visibleItems: [Item] = []
    /// Folders open in the sidebar outline.
    var expanded: Set<String> = [""]
    /// How far search has got opening the archives inside this one.
    private(set) var searchingInside: (done: Int, total: Int)?

    /// The Quick Look panel's file, and the files it can page through.
    var quickLookURL: URL?
    private(set) var quickLookURLs: [URL] = []

    /// The most recent job started here; `jobs` has them all.
    private(set) var job: Job?
    private(set) var jobs: [Job] = []
    var passwordRequest: PasswordRequest?
    var alert: String?
    /// Bumped to put the cursor in the search field.
    private(set) var searchFocusRequest = 0
    /// The item being renamed, for the rename sheet.
    var renaming: Item?
    /// Files opened in other apps from this archive and changed there, waiting to go back in.
    private(set) var changedOutside: [EditedFile] = []

    /// Private working folder: unwrapped tars, nested archives, previews, drags.
    @ObservationIgnored let scratch = FileManager.default.temporaryDirectory
        .appending(path: "Zipling/\(UUID().uuidString)")
    @ObservationIgnored private var quarantine: Data?
    @ObservationIgnored private var refreshGeneration = 0
    /// Archives inside other levels, opened for search or browsing, by `innerKey`.
    @ObservationIgnored private var inner: [String: Level] = [:]
    @ObservationIgnored private var innerHits: [String: [Item]] = [:]
    @ObservationIgnored private var unreadable: Set<String> = []
    @ObservationIgnored private var indexTask: Task<Void, Never>?
    @ObservationIgnored private var materialized: [String: URL] = [:]
    @ObservationIgnored private var typed = ""
    @ObservationIgnored private var typedAt = Date.distantPast
    @ObservationIgnored private var editedFiles: [EditedFile] = []
    @ObservationIgnored private var editWatch: Timer?

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
            levelChanged(folder: "")
            #if DEBUG
            // Website screenshots: `open -a Zipling Backup.zlg --args -ShowItem Photos/Beach.jpg`.
            if let path = UserDefaults.standard.string(forKey: "ShowItem"), let item = levels[0].byID[path] {
                show(folder: item.parent)
                selection = [path]
            }
            #endif
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

    /// Stops background work and removes the private working folder (the window closed).
    func close() {
        editWatch?.invalidate()
        indexTask?.cancel()
        for job in jobs where job.isRunning { job.cancel() }
        try? FileManager.default.removeItem(at: scratch)
    }

    private static func makeLevel(name: String, archive: OpenedArchive) async -> Level {
        let entries = archive.listing.entries
        return await Task.detached(priority: .userInitiated) {
            let (children, everything) = buildTree(entries)
            let byID = Dictionary(everything.map { ($0.id, $0) }, uniquingKeysWith: { a, _ in a })
            var subfolders: [String: [String]] = [:]
            for (parent, items) in children {
                let folders = items.filter(\.isFolder).map(\.id)
                if !folders.isEmpty { subfolders[parent] = folders.sorted { $0.localizedStandardCompare($1) == .orderedAscending } }
            }
            return Level(name: name, archive: archive, children: children, everything: everything, byID: byID,
                         subfolders: subfolders)
        }.value
    }

    private func askPassword(
        for name: String, wrong: Bool, retry: @escaping @MainActor (String) -> Void,
        giveUp: @escaping @MainActor () -> Void = {}
    ) {
        passwordRequest = PasswordRequest(archiveName: name, wrong: wrong, retry: retry, giveUp: giveUp)
    }

    // MARK: Listing

    /// Recomputes `visibleItems`. Big folders and searches are sorted off the main
    /// thread, so typing stays smooth with hundreds of thousands of entries.
    private func refresh() {
        guard let level else {
            visibleItems = []
            return
        }
        refreshGeneration += 1
        let generation = refreshGeneration
        let search = search
        let order = sortOrder
        var pool = search.isEmpty ? level.children[folder] ?? [] : level.everything
        if !search.isEmpty, searchInside {
            let base = level.archive.source.path
            for item in level.everything where item.isArchive {
                pool += innerHits[Self.innerKey(base, item.id)] ?? []
            }
            indexInside()
        }
        if pool.count < 2_000 {
            visibleItems = Self.arrange(pool, search: search, order: order)
            return
        }
        Task.detached(priority: .userInitiated) {
            let arranged = Self.arrange(pool, search: search, order: order)
            await MainActor.run { [weak self] in
                guard let self, self.refreshGeneration == generation else { return }
                self.visibleItems = arranged
            }
        }
    }

    /// Search matches, sorted, folders first.
    nonisolated static func arrange(_ items: [Item], search: String, order: [KeyPathComparator<Item>]) -> [Item] {
        let matched = search.isEmpty ? items : items.filter { $0.name.localizedStandardContains(search) }
        let sorted = matched.sorted(using: order)
        return sorted.filter(\.isFolder) + sorted.filter { !$0.isFolder }
    }

    private func folderChanged() {
        // Reveal the folder in the sidebar outline.
        var p = folder
        while let slash = p.lastIndex(of: "/") {
            p = String(p[..<slash])
            expanded.insert(p)
        }
        expanded.insert("")
        refresh()
    }

    private func levelChanged(folder: String) {
        indexTask?.cancel()
        indexTask = nil
        searchingInside = nil
        search = ""
        selection = []
        expanded = [""]
        if self.folder == folder { folderChanged() } else { self.folder = folder }
    }

    // MARK: Navigation

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

    func subfolders(of path: String) -> [String] {
        level?.subfolders[path] ?? []
    }

    func item(_ id: Item.ID) -> Item? {
        if let hit = level?.byID[id] { return hit }
        return visibleItems.first { $0.id == id }
    }

    var selectedItem: Item? {
        selection.count == 1 ? selection.first.flatMap(item) : nil
    }

    /// The selection in list order.
    var selectedItems: [Item] {
        visibleItems.filter { selection.contains($0.id) }
    }

    func show(folder path: String) {
        search = ""
        folder = path
        selection = []
    }

    func enter(_ item: Item) {
        guard item.isFolder else { return }
        if item.inArchive != nil { return reveal(item) }
        show(folder: item.id)
    }

    func goUp() {
        if !search.isEmpty {
            search = ""
        } else if let slash = folder.lastIndex(of: "/") {
            let child = folder
            folder = String(folder[..<slash])
            selection = [child]
        } else if !folder.isEmpty {
            let child = folder
            folder = ""
            selection = [child]
        } else if levels.count > 1 {
            leaveTo(level: levels.count - 2)
        }
    }

    /// Returns to an outer archive (0 is the window's own).
    func leaveTo(level index: Int) {
        guard index < levels.count - 1 else { return }
        levels.removeSubrange((index + 1)...)
        levelChanged(folder: levels[index].folder)
    }

    var canGoUp: Bool { !folder.isEmpty || !search.isEmpty || levels.count > 1 }

    func focusSearch() {
        searchFocusRequest += 1
    }

    /// Finder's type-to-select: typing a name's first letters selects it.
    func typeSelect(_ characters: String) -> Bool {
        guard !characters.isEmpty, characters != " ",
              !characters.unicodeScalars.contains(where: { CharacterSet.controlCharacters.contains($0) || $0.value >= 0xF700 })
        else { return false }
        if Date().timeIntervalSince(typedAt) > 1 { typed = "" }
        typed += characters
        typedAt = Date()
        let prefix = typed
        if let match = visibleItems.first(where: { $0.name.lowercased().hasPrefix(prefix.lowercased()) })
            ?? visibleItems.first(where: { $0.name.localizedStandardCompare(prefix) != .orderedAscending }) {
            selection = [match.id]
        }
        return true
    }

    // MARK: Nested archives

    private static func innerKey(_ base: String, _ id: String) -> String { base + "\u{1}" + id }

    /// Opens an archive stored inside the current one, as a new level.
    func openNested(_ item: Item, password: String? = nil, then: (@MainActor () -> Void)? = nil) {
        guard let level, openingNested == nil else { return }
        let key = Self.innerKey(level.archive.source.path, item.id)
        if password == nil, let cached = inner[key] {
            push(cached, then: then)
            return
        }
        openingNested = item.name
        Task {
            defer { openingNested = nil }
            do {
                let opened = try await Archives.openNested(
                    item.id, in: level.archive, scratch: scratch, quarantine: quarantine, password: password)
                let inner = await Self.makeLevel(name: item.name, archive: opened)
                self.inner[key] = inner
                push(inner, then: then)
            } catch let p as PasswordRequired {
                askPassword(for: item.name, wrong: p.wrong) { password in
                    self.openNested(item, password: password, then: then)
                }
            } catch {
                alert = "Couldn't open “\(item.name)”: \(error.localizedDescription)"
            }
        }
    }

    private func push(_ inner: Level, then: (@MainActor () -> Void)?) {
        levels[levels.count - 1].folder = folder
        levels.append(inner)
        levelChanged(folder: "")
        then?()
    }

    /// Opens the archive a search hit is in, at the hit.
    func reveal(_ hit: Item) {
        guard let archiveID = hit.inArchive, let archive = level?.byID[archiveID] else { return }
        let path = hit.path, parent = hit.parent
        openNested(archive) { [weak self] in
            self?.show(folder: parent)
            self?.selection = [path]
        }
    }

    /// Opens the archives inside this one in the background, smallest first, so search
    /// can see into them. Results appear as each one is read.
    private func indexInside() {
        guard indexTask == nil, let level else { return }
        let base = level.archive.source.path
        let candidates = level.everything
            .filter { $0.isArchive && $0.size <= Self.searchInsideLimit }
            .filter { inner[Self.innerKey(base, $0.id)] == nil && !unreadable.contains(Self.innerKey(base, $0.id)) }
            .sorted { $0.size < $1.size }
            .prefix(500)
        guard !candidates.isEmpty else { return }
        searchingInside = (0, candidates.count)
        indexTask = Task {
            for (n, item) in candidates.enumerated() {
                if Task.isCancelled { return }
                let key = Self.innerKey(base, item.id)
                if let opened = try? await Archives.openNested(
                    item.id, in: level.archive, scratch: scratch, quarantine: quarantine, password: nil) {
                    let level = await Self.makeLevel(name: item.name, archive: opened)
                    inner[key] = level
                    innerHits[key] = level.everything.map { entry in
                        var hit = entry
                        hit.id = key + "\u{1}" + entry.path
                        hit.inArchive = item.id
                        return hit
                    }
                } else {
                    unreadable.insert(key)
                }
                if Task.isCancelled { return }
                searchingInside = (n + 1, candidates.count)
                if !search.isEmpty, searchInside { refresh() }
            }
            searchingInside = nil
            indexTask = nil
        }
    }

    /// The archive an item lives in.
    private func source(of item: Item) -> OpenedArchive? {
        guard let level else { return nil }
        guard let archiveID = item.inArchive else { return level.archive }
        return inner[Self.innerKey(level.archive.source.path, archiveID)]?.archive
    }

    /// Location of a search hit, for the list: "photos.zip › 2024/May".
    func location(of item: Item) -> String {
        guard let archiveID = item.inArchive else { return item.parent }
        let archive = (archiveID as NSString).lastPathComponent
        return item.parent.isEmpty ? archive : "\(archive) › \(item.parent)"
    }

    // MARK: File-name encoding

    /// Whether the current archive's names can be re-read with another encoding:
    /// formats whose names don't record one, read by The Unarchiver.
    var canChangeEncoding: Bool {
        guard let archive = level?.archive, archive.backend != .arca, Unar.isAvailable else { return false }
        return archive.backend == .unar || Archives.legacyNameFormats.contains((archive.listing.format ?? "").lowercased())
    }

    /// The encoding picked by hand, or nil when it's detected.
    var chosenEncoding: String? {
        guard let archive = level?.archive, archive.encodingChosen else { return nil }
        return archive.encoding
    }

    /// The encoding the names were detected in, when it isn't plain Unicode.
    var detectedEncoding: String? {
        guard let archive = level?.archive, !archive.encodingChosen, archive.backend == .unar else { return nil }
        return archive.encoding
    }

    /// Reads the current archive's names again with the given encoding (nil: detect it).
    func reopen(encoding: String?) {
        guard let level, encoding != chosenEncoding else { return }
        Task {
            do {
                let archive = try await Archives.open(
                    level.archive.source, password: level.archive.password, scratch: scratch, encoding: encoding)
                levels[levels.count - 1] = await Self.makeLevel(name: level.name, archive: archive)
                materialized = [:]
                levelChanged(folder: "")
            } catch {
                alert = "Couldn't read the names as \(encoding.map(Unar.label) ?? "detected"): \(error.localizedDescription)"
            }
        }
    }

    // MARK: Previews

    /// The item as a real file in the private folder, extracted once and reused: for
    /// the preview pane, Quick Look, dragging out and opening in other apps.
    func materialize(_ item: Item) async throws -> URL {
        guard let archive = source(of: item) else { throw EngineError(message: "This item is no longer available.") }
        let key = archive.source.path + "\u{1}" + item.path
        if let url = materialized[key], Placement.exists(url) { return url }
        let dir = scratch.appending(path: "items/\(UUID().uuidString)")
        let task = EngineTask(), cancel = CancelToken()
        let quarantine = quarantine
        try await withTaskCancellationHandler {
            _ = try await Archives.extract(archive, to: dir, only: [item.path], task: task, cancel: cancel) { _, _ in }
            _ = try SafeExtraction.finalize(dir, quarantine: quarantine)
        } onCancel: {
            task.cancel()
            cancel.cancel()
        }
        if Task.isCancelled { throw CancellationError() }
        let url = dir.appending(path: item.path)
        guard Placement.exists(url) else { throw EngineError(message: "“\(item.name)” couldn't be read.") }
        materialized[key] = url
        return url
    }

    /// Space bar: shows the selection in Quick Look, or closes it.
    func toggleQuickLook() {
        if quickLookURL != nil {
            quickLookURL = nil
            return
        }
        previewSelectionInQuickLook()
    }

    private func previewSelectionInQuickLook() {
        let items = selectedItems.filter { !$0.isFolder }.prefix(64)
        guard !items.isEmpty else { return }
        Task {
            var urls: [URL] = []
            for item in items {
                do {
                    urls.append(try await materialize(item))
                } catch let p as PasswordRequired {
                    askPassword(for: level?.name ?? name, wrong: p.wrong) { password in
                        self.setPassword(password)
                        self.previewSelectionInQuickLook()
                    }
                    return
                } catch {
                    continue
                }
            }
            guard !urls.isEmpty else { return }
            quickLookURLs = urls
            quickLookURL = urls[0]
        }
    }

    private func selectionChanged() {
        // Like Finder, an open Quick Look panel follows the selection.
        if quickLookURL != nil { previewSelectionInQuickLook() }
    }

    var hasPassword: Bool { level?.archive.password != nil }

    /// Asks for the current archive's password, then runs `then`.
    func unlock(then: @escaping @MainActor () -> Void) {
        askPassword(for: level?.name ?? name, wrong: false) { [weak self] password in
            self?.setPassword(password)
            then()
        }
    }

    /// Shares items from inside the archive (Mail, Messages, AirDrop…), extracting them first.
    func share(_ ids: Set<Item.ID>) {
        let items = ids.compactMap(item)
        guard !items.isEmpty else { return }
        Task {
            do {
                var urls: [URL] = []
                for item in items { urls.append(try await materialize(item)) }
                guard let view = NSApp.keyWindow?.contentView else { return }
                let picker = NSSharingServicePicker(items: urls)
                picker.show(relativeTo: NSRect(x: view.bounds.midX, y: view.bounds.maxY - 60, width: 1, height: 1),
                            of: view, preferredEdge: .minY)
            } catch {
                alert = "Couldn't share: \(error.localizedDescription)"
            }
        }
    }

    // MARK: Actions

    /// Double-click or ⌘↓: open folders and archives inside, and open other files with their default app.
    func activate(_ ids: Set<Item.ID>) {
        guard let id = ids.first, let item = item(id) else { return }
        if item.inArchive != nil {
            reveal(item)
        } else if item.isFolder {
            enter(item)
        } else if item.isArchive {
            openNested(item)
        } else {
            openWithDefaultApp(item)
        }
    }

    func openWithDefaultApp(_ item: Item) {
        Task {
            do {
                NSWorkspace.shared.open(try await materialize(item))
            } catch let p as PasswordRequired {
                askPassword(for: level?.name ?? name, wrong: p.wrong) { password in
                    self.setPassword(password)
                    self.openWithDefaultApp(item)
                }
            } catch {
                alert = "Couldn't open “\(item.name)”: \(error.localizedDescription)"
            }
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
            ? "Choose where to extract “\(level?.name ?? name)”"
            : "Choose where to extract \(ids.count == 1 ? "1 item" : "\(ids.count) items")"
        guard panel.runModal() == .OK, let dest = panel.url else { return }
        extract(ids, to: dest, then: Preferences.revealAfterExtract ? .reveal : .nothing)
    }

    /// Extract Here: next to the archive, everything in one place. One item at the
    /// top comes out as it is; several go in a folder named after the archive.
    func extractHere() {
        guard let level else { return }
        extract([], to: url.deletingLastPathComponent(),
                then: Preferences.revealAfterExtract ? .reveal : .nothing,
                layout: .smart(folder: Archives.baseName(level.name)))
    }

    enum AfterExtract { case reveal, open, nothing }

    func extract(
        _ ids: Set<Item.ID>, to dest: URL, then after: AfterExtract,
        layout: Extraction.Layout = .asIs, conflicts: Conflicts = Conflicts()
    ) {
        guard let level else { return }
        let items = ids.compactMap(item)
        // Search hits inside other archives come out of their own archive.
        let hits = Dictionary(grouping: items.filter { $0.inArchive != nil }, by: { $0.inArchive! })
        // Selecting a folder and something inside it means just the folder.
        let picked = items.filter { $0.inArchive == nil }.map(\.id).sorted()
        let roots = picked.filter { p in !picked.contains { p != $0 && p.hasPrefix($0 + "/") } }
        var groups: [(OpenedArchive, [String])] = []
        if !roots.isEmpty || hits.isEmpty { groups.append((level.archive, roots)) }
        for (archiveID, found) in hits.sorted(by: { $0.key < $1.key }) {
            guard let archive = inner[Self.innerKey(level.archive.source.path, archiveID)]?.archive else { continue }
            groups.append((archive, found.map(\.path)))
        }
        let count = roots.count + hits.values.reduce(0) { $0 + $1.count }
        let job = Job(title: count == 0
            ? "Extracting “\(level.name)”…"
            : "Extracting \(count == 1 ? "“\(items.first?.name ?? "")”" : "\(count) items")…")
        start(job)
        let quarantine = quarantine
        let wholeArchive = levels.count == 1 && count == 0
        let original = url
        Task {
            do {
                var placed: [URL] = []
                var summaries: [String] = []
                var complete = true
                for (archive, paths) in groups {
                    let outcome = try await Extraction.run(
                        archive, only: paths, to: dest, layout: layout, job: job, quarantine: quarantine, conflicts: conflicts)
                    placed += outcome.placed
                    summaries.append(outcome.summary)
                    complete = complete && outcome.complete
                }
                var summary = summaries.joined(separator: " ")
                if level.archive.backend == .arca { summary += " Every byte was verified." }
                // Only once everything came out, checksums and all.
                if wholeArchive, complete, Preferences.trashAfterExtract, Extraction.checksDataOf(level.archive),
                   let trashed = try? Extraction.trash(original) {
                    summary += trashed.count > 1
                        ? " The archive's \(trashed.count) parts were moved to the Trash."
                        : " The archive was moved to the Trash."
                }
                job.finish(nil, outcome: summary, placed: placed)
                switch after {
                case .reveal: NSWorkspace.shared.activateFileViewerSelecting(placed)
                case .open: if let first = placed.first { NSWorkspace.shared.open(first) }
                case .nothing: break
                }
            } catch let p as PasswordRequired {
                job.finish(EngineError(message: "cancelled"))
                dismiss(job)
                askPassword(for: level.name, wrong: p.wrong) { password in
                    self.setPassword(password)
                    self.extract(ids, to: dest, then: after, layout: layout, conflicts: conflicts)
                }
            } catch {
                job.finish(error)
            }
        }
    }

    func verify() {
        guard let level else { return }
        let job = Job(title: "Verifying “\(level.name)”…")
        start(job)
        let archive = level.archive
        Task {
            do {
                let result = try await Archives.verify(archive, task: job.task, cancel: job.cancelToken,
                                                       progress: job.progressHandler)
                job.finish(nil, outcome: archive.backend == .arca
                    ? "All \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) decoded and matched their BLAKE3 hashes in \(Format.duration(result.seconds))."
                    : "All \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) decoded and passed the archive's checks in \(Format.duration(result.seconds)).")
            } catch let p as PasswordRequired {
                job.finish(EngineError(message: "cancelled"))
                dismiss(job)
                askPassword(for: level.name, wrong: p.wrong) { password in
                    self.setPassword(password)
                    self.verify()
                }
            } catch {
                job.finish(error)
            }
        }
    }

    private func start(_ job: Job) {
        self.job = job
        jobs.insert(job, at: 0)
        JobCenter.shared.track(job)
        // Changes to the archive report briefly, then step aside; extractions keep
        // their Show in Finder, and anything that failed stays until dismissed.
        Task { [weak self, weak job] in
            while let job, job.isRunning { try? await Task.sleep(for: .milliseconds(250)) }
            try? await Task.sleep(for: .seconds(8))
            guard let self, let job, case .finished = job.phase, job.placed.isEmpty else { return }
            self.jobs.removeAll { $0 === job }
            if self.job === job { self.job = self.jobs.first }
        }
    }

    /// Forgets a password that turned out wrong, so the archive asks again.
    func forgetPassword() {
        guard !levels.isEmpty else { return }
        levels[levels.count - 1].archive.password = nil
        materialized = [:]
    }

    func dismiss(_ job: Job) {
        jobs.removeAll { $0 === job }
        if self.job === job { self.job = jobs.first }
        JobCenter.shared.dismiss(job)
    }

    private func setPassword(_ password: String) {
        guard !levels.isEmpty else { return }
        levels[levels.count - 1].archive.password = password
        materialized = [:]
    }

    // MARK: Editing

    /// A file opened in its own app from the archive.
    struct EditedFile: Identifiable, Equatable {
        let id = UUID()
        /// Where it is on disk while it's being edited.
        let url: URL
        /// Its folder in the archive.
        let folder: String
        var modified: Date
        var name: String { url.lastPathComponent }
    }

    /// Why this archive can't be changed, or nil when it can.
    var editProblem: String? {
        guard let level else { return "The archive isn't open yet." }
        if levels.count > 1 { return "An archive inside another one can't be changed here. Extract it, then open it." }
        if case .failure(let error) = Editing.method(for: level.archive, url: url) { return error.message }
        return nil
    }

    var canEdit: Bool { editProblem == nil }

    /// Adds files and folders to the folder on screen (or `folder`), replacing any with the same name.
    func add(_ files: [URL], into folder: String? = nil) {
        // Items dragged out of this window and back in are already here.
        put(files.filter { !$0.path.hasPrefix(scratch.path + "/items/") }, into: folder ?? (search.isEmpty ? self.folder : ""))
    }

    private func put(_ files: [URL], into target: String, verb: String = "Adding") {
        guard !files.isEmpty else { return }
        let title = files.count == 1 ? "\(verb) “\(files[0].lastPathComponent)”…" : "\(verb) \(files.count) items…"
        let select = files.map { target.isEmpty ? $0.lastPathComponent : "\(target)/\($0.lastPathComponent)" }
        change(.add(files, into: target), title: title, select: Set(select))
    }

    func chooseFilesToAdd() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        panel.prompt = "Add"
        panel.message = "Choose files and folders to add to “\(folder.isEmpty ? levels.first?.name ?? name : (folder as NSString).lastPathComponent)”"
        if panel.runModal() == .OK { add(panel.urls) }
    }

    func delete(_ ids: Set<Item.ID>) {
        let items = ids.compactMap(item).filter { $0.inArchive == nil }
        guard !items.isEmpty else { return }
        let alert = NSAlert()
        alert.messageText = items.count == 1
            ? "Delete “\(items[0].name)” from the archive?"
            : "Delete \(items.count) items from the archive?"
        alert.informativeText = "This changes “\(url.lastPathComponent)” and can't be undone."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }
        let paths = items.map(\.path)
        change(.delete(paths), title: items.count == 1 ? "Deleting “\(items[0].name)”…" : "Deleting \(items.count) items…")
    }

    /// Renames an item; the reason it can't be renamed comes back otherwise.
    @discardableResult
    func rename(_ item: Item, to newName: String) -> String? {
        let name = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard name != item.name else { return nil }
        if name.isEmpty || name.contains("/") || name == "." || name == ".." { return "Names can't be empty or contain “/”." }
        let siblings = level?.children[item.parent] ?? []
        if siblings.contains(where: { $0.name == name }) { return "There's already an item named “\(name)” here." }
        let path = item.parent.isEmpty ? name : "\(item.parent)/\(name)"
        change(.rename(item.path, to: path), title: "Renaming “\(item.name)”…", select: [path])
        return nil
    }

    func newFolder() {
        let parent = search.isEmpty ? folder : ""
        let names = Set((level?.children[parent] ?? []).map(\.name))
        var name = "untitled folder", n = 2
        while names.contains(name) {
            name = "untitled folder \(n)"
            n += 1
        }
        let path = parent.isEmpty ? name : "\(parent)/\(name)"
        change(.newFolder(path), title: "Adding a folder…", select: [path]) { [weak self] in
            if let item = self?.level?.byID[path] { self?.renaming = item }
        }
    }

    /// Opens a file in its own app; when it's saved there, Zipling offers to put it back.
    func editInApp(_ item: Item, open: @escaping @MainActor (URL) -> Void = { NSWorkspace.shared.open($0) }) {
        if let problem = editProblem {
            alert = problem
            return
        }
        Task {
            do {
                let preview = try await materialize(item)
                let dir = scratch.appending(path: "edit/\(UUID().uuidString)")
                try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                let copy = dir.appending(path: item.name)
                try FileManager.default.copyItem(at: preview, to: copy)
                editedFiles.append(EditedFile(url: copy, folder: item.parent, modified: Self.modified(copy)))
                watchEdits()
                open(copy)
            } catch {
                alert = "Couldn't open “\(item.name)”: \(error.localizedDescription)"
            }
        }
    }

    func saveBack(_ file: EditedFile) {
        changedOutside.removeAll { $0.id == file.id }
        put([file.url], into: file.folder, verb: "Updating")
    }

    func ignoreChanges(_ file: EditedFile) {
        changedOutside.removeAll { $0.id == file.id }
    }

    private func watchEdits() {
        guard editWatch == nil else { return }
        editWatch = Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.checkEdits() }
        }
    }

    /// Looks for saves in other apps (every second while anything is open for editing).
    func checkEdits() {
        for i in editedFiles.indices {
            let now = Self.modified(editedFiles[i].url)
            guard now > editedFiles[i].modified else { continue }
            editedFiles[i].modified = now
            if !changedOutside.contains(where: { $0.url == editedFiles[i].url }) { changedOutside.append(editedFiles[i]) }
        }
    }

    /// Read from disk each time: a URL caches its resource values, so it would never see a save.
    private static func modified(_ url: URL) -> Date {
        (try? FileManager.default.attributesOfItem(atPath: url.path))?[.modificationDate] as? Date ?? .distantPast
    }

    private func change(
        _ change: Editing.Change, title: String, select: Set<String> = [], then: (@MainActor () -> Void)? = nil
    ) {
        guard let level, levels.count == 1 else { return }
        if let problem = editProblem {
            alert = problem
            return
        }
        guard case .success(let method) = Editing.method(for: level.archive, url: url) else { return }
        // New entries in an encrypted archive need its password.
        if level.archive.listing.encrypted == true, level.archive.password == nil {
            return unlock { [weak self] in self?.change(change, title: title, select: select, then: then) }
        }
        let job = Job(title: title)
        start(job)
        let original = url
        Task {
            do {
                try await Editing.apply(change, to: level.archive, at: original, method: method, task: job.task,
                                        cancel: job.cancelToken, progress: job.progressHandler)
                try await reload(select: select)
                job.finish(nil, outcome: "“\(original.lastPathComponent)” was updated.")
                then?()
            } catch let p as PasswordRequired {
                job.finish(EngineError(message: "cancelled"))
                dismiss(job)
                askPassword(for: level.name, wrong: p.wrong) { password in
                    self.setPassword(password)
                    self.change(change, title: title, select: select, then: then)
                }
            } catch {
                job.finish(error)
            }
        }
    }

    /// Reads the archive again after a change, staying in the same folder.
    private func reload(select: Set<String>) async throws {
        guard let level = levels.first else { return }
        let archive = try await Archives.open(url, password: level.archive.password, scratch: scratch,
                                              encoding: level.archive.encodingChosen ? level.archive.encoding : nil)
        let reopened = await Self.makeLevel(name: level.name, archive: archive)
        let stay = reopened.byID[folder]?.isFolder == true ? folder : ""
        levels = [reopened]
        materialized = [:]
        inner = [:]
        innerHits = [:]
        unreadable = []
        levelChanged(folder: stay)
        selection = select.filter { reopened.byID[$0] != nil }
    }

    // MARK: Tree

    /// Groups entries by parent folder and totals folder sizes. Folders that the
    /// archive doesn't list explicitly are synthesised.
    nonisolated static func buildTree(_ entries: [ArchiveEntry]) -> ([String: [Item]], [Item]) {
        var folders: [String: Item] = [:]
        var files: [Item] = []
        files.reserveCapacity(entries.count)

        func parentOf(_ path: String) -> String {
            guard let slash = path.lastIndex(of: "/") else { return "" }
            return String(path[..<slash])
        }
        // Folder dates: their own where the archive lists one, else their newest file's.
        var dated: [String: Int64] = [:]
        var newest: [String: Int64] = [:]
        func folder(_ path: String) {
            if folders[path] == nil {
                folders[path] = Item(
                    id: path, path: path, name: (path as NSString).lastPathComponent, parent: parentOf(path),
                    isFolder: true, kind: "dir", size: 0, packed: 0,
                    modified: .distantPast, transform: "none", codec: "store")
            }
        }

        for e in entries {
            if e.isDirectory {
                folder(e.path)
                if e.mtime > 0 { dated[e.path] = e.mtime }
                continue
            }
            let packedKnown = e.packedKnown ?? true
            files.append(Item(
                id: e.path, path: e.path, name: e.name, parent: e.parent, isFolder: false, kind: e.kind,
                size: e.size, packed: e.packed, packedKnown: packedKnown,
                modified: Date(timeIntervalSince1970: TimeInterval(e.mtime)),
                transform: e.transform, codec: e.codec, encrypted: e.encrypted ?? false))
            var p = e.parent
            while !p.isEmpty {
                folder(p)
                newest[p] = max(newest[p] ?? 0, e.mtime)
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

        for (path, var f) in folders {
            let time = dated[path] ?? newest[path] ?? 0
            f.modified = Date(timeIntervalSince1970: TimeInterval(time))
            folders[path] = f
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

/// An item dragged out of the archive to Finder or another app. The receiver gets
/// the real file or folder, extracted when it's dropped rather than when the drag starts.
struct ArchiveDrag: Transferable {
    let item: Item
    let model: ArchiveModel

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .folder) { drag in
            SentTransferredFile(try await drag.model.materialize(drag.item))
        }
        .exportingCondition { $0.item.isFolder }
        .suggestedFileName { $0.item.name }
        FileRepresentation(exportedContentType: .data) { drag in
            SentTransferredFile(try await drag.model.materialize(drag.item))
        }
        .exportingCondition { !$0.item.isFolder }
        .suggestedFileName { $0.item.name }
    }
}
