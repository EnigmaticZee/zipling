import SwiftUI

@main
struct ZiplingApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        Window("Zipling", id: AppRouter.homeWindow) {
            HomeView()
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentSize)
        .commands { AppCommands() }

        WindowGroup("Archive", for: URL.self) { $url in
            if let url {
                ArchiveWindow(url: url)
            }
        }
        .defaultSize(width: 1180, height: 700)
    }
}

struct AppCommands: Commands {
    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Compress Files…") { AppRouter.shared.chooseFilesToCompress() }
                .keyboardShortcut("n")
            Button("Open Archive…") { AppRouter.shared.chooseArchive() }
                .keyboardShortcut("o")
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    /// Archives double-clicked in Finder, or files dropped on the Dock icon.
    func application(_ application: NSApplication, open urls: [URL]) {
        AppRouter.shared.open(urls)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
