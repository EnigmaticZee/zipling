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
        panel.allowedContentTypes = [.ziplingArchive]
        panel.allowsMultipleSelection = true
        panel.message = "Choose a Zipling archive to open"
        if panel.runModal() == .OK { open(panel.urls) }
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

    private static func isArchive(_ url: URL) -> Bool {
        url.pathExtension.lowercased() == "zlg"
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
