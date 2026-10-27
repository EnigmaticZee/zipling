import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Routes "open these files" from anywhere (Finder, Dock, drops, menus) to the
/// right window: archives get a browser window, everything else is compressed.
@MainActor
@Observable
final class AppRouter {
    static let shared = AppRouter()
    static let homeWindow = "home"

    /// Files waiting for the compress sheet in the home window.
    var compressRequest: CompressRequest?
    private(set) var recents: [URL] = RecentArchives.load()

    @ObservationIgnored private var openWindow: OpenWindowAction?
    @ObservationIgnored private var queued: [URL] = []

    /// Called by the home window once SwiftUI can open windows.
    func attach(_ action: OpenWindowAction) {
        openWindow = action
        let pending = queued
        queued = []
        open(pending)
    }

    func open(_ urls: [URL]) {
        guard let openWindow else {
            queued += urls
            return
        }
        let archives = urls.filter(Self.isArchive)
        let others = urls.filter { !Self.isArchive($0) }
        for url in archives {
            openWindow(value: url)
            RecentArchives.add(url)
        }
        recents = RecentArchives.load()
        if !others.isEmpty {
            openWindow(id: Self.homeWindow)
            compressRequest = CompressRequest(items: others)
        }
    }

    func noteCreated(_ archive: URL) {
        RecentArchives.add(archive)
        recents = RecentArchives.load()
    }

    func chooseArchive() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = Self.archiveTypes
        panel.allowsMultipleSelection = true
        panel.message = "Choose an archive to open"
        if panel.runModal() == .OK { open(panel.urls) }
    }

    /// Files and folders to compress, from Finder, the Dock or the menu bar.
    func compress(_ urls: [URL]) {
        guard !urls.isEmpty else { return }
        NSApp.activate()
        openWindow?(id: Self.homeWindow)
        compressRequest = CompressRequest(items: urls)
    }

    func showHome() {
        openWindow?(id: Self.homeWindow)
    }

    /// Extract Here for each of several archives, in the background.
    func extractHere(_ urls: [URL]) {
        let archives = urls.filter(Self.isArchive)
        guard !archives.isEmpty else { return }
        openWindow?(id: Self.homeWindow)
        Task { await BatchExtraction.run(archives) }
    }

    func chooseArchivesToExtract() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = Self.archiveTypes
        panel.allowsMultipleSelection = true
        panel.prompt = "Extract"
        panel.message = "Choose archives to extract. Each one comes out next to itself."
        if panel.runModal() == .OK { extractHere(panel.urls) }
    }

    func chooseFilesToCompress() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = true
        panel.prompt = "Compress"
        panel.message = "Choose files and folders to compress"
        if panel.runModal() == .OK, !panel.urls.isEmpty {
            openWindow?(id: Self.homeWindow)
            compressRequest = CompressRequest(items: panel.urls)
        }
    }

    private static let archiveTypes: [UTType] = [.ziplingArchive, .archive, .diskImage, .zip, .gzip, .bz2]
        + Archives.archiveExtensions.compactMap { UTType(filenameExtension: $0) }

    /// Archives open in a browser window; folders and other files go to Compress.
    static func isArchive(_ url: URL) -> Bool {
        if (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true { return false }
        if Archives.looksLikeArchive(url.lastPathComponent) || Archives.isZipling(url) { return true }
        guard let type = try? url.resourceValues(forKeys: [.contentTypeKey]).contentType else { return false }
        return type.conforms(to: .archive) || type.conforms(to: .diskImage)
    }
}

struct CompressRequest: Identifiable {
    let id = UUID()
    let items: [URL]
}

/// Recently opened or created archives, newest first.
enum RecentArchives {
    private static let key = "recentArchives"

    static func load() -> [URL] {
        let paths = UserDefaults.standard.stringArray(forKey: key) ?? []
        return paths.map { URL(filePath: $0) }.filter { FileManager.default.fileExists(atPath: $0.path) }
    }

    static func add(_ url: URL) {
        var paths = UserDefaults.standard.stringArray(forKey: key) ?? []
        paths.removeAll { $0 == url.path }
        paths.insert(url.path, at: 0)
        UserDefaults.standard.set(Array(paths.prefix(8)), forKey: key)
    }
}
