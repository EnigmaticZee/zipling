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
        .defaultSize(width: 1240, height: 720)

        Settings {
            SettingsView()
        }
    }
}

struct AppCommands: Commands {
    @FocusedValue(\.archiveModel) private var model

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Compress Files…") { AppRouter.shared.chooseFilesToCompress() }
                .keyboardShortcut("n")
            Button("Open Archive…") { AppRouter.shared.chooseArchive() }
                .keyboardShortcut("o")
            Button("Extract Archives…") { AppRouter.shared.chooseArchivesToExtract() }
                .keyboardShortcut("o", modifiers: [.command, .shift])
        }
        CommandGroup(after: .newItem) {
            Divider()
            Button("Extract Here") { model?.extractHere() }
                .keyboardShortcut("e", modifiers: [.command, .shift])
                .disabled(model?.listing == nil)
            Button(model?.selection.isEmpty == false ? "Extract Selected…" : "Extract All…") {
                if let model { model.extractInteractively(model.selection) }
            }
            .keyboardShortcut("e")
            .disabled(model?.listing == nil)
            Button("Quick Look") { model?.toggleQuickLook() }
                .keyboardShortcut("y")
                .disabled(model?.selection.isEmpty ?? true)
            Button("Verify Archive") { model?.verify() }
                .disabled(model?.listing == nil)
            Divider()
            Button("Add Files…") { model?.chooseFilesToAdd() }
                .keyboardShortcut("a", modifiers: [.command, .option])
                .disabled(!(model?.canEdit ?? false))
            Button("New Folder") { model?.newFolder() }
                .keyboardShortcut("n", modifiers: [.command, .shift])
                .disabled(!(model?.canEdit ?? false))
            Button("Rename…") { model?.renaming = model?.selectedItem }
                .disabled(!(model?.canEdit ?? false) || model?.selectedItem == nil)
            Button("Delete from Archive…") { if let model { model.delete(model.selection) } }
                .keyboardShortcut(.delete)
                .disabled(!(model?.canEdit ?? false) || (model?.selection.isEmpty ?? true))
        }
        CommandGroup(after: .textEditing) {
            Button("Find in Archive") { model?.focusSearch() }
                .keyboardShortcut("f")
                .disabled(model == nil)
        }
        CommandMenu("Go") {
            Button("Back") { model?.goUp() }
                .keyboardShortcut("[")
                .disabled(!(model?.canGoUp ?? false))
            Button("Enclosing Folder") { model?.goUp() }
                .keyboardShortcut(.upArrow)
                .disabled(!(model?.canGoUp ?? false))
            Button("Open") { if let model { model.activate(model.selection) } }
                .keyboardShortcut(.downArrow)
                .disabled(model?.selection.isEmpty ?? true)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    private let services = ServiceProvider()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.servicesProvider = services
        NSUpdateDynamicServices()
        // Not while the test suite runs inside the app.
        if ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] == nil {
            MenuBarDropZone.shared.update()
        }
    }

    /// Archives double-clicked in Finder, or files dropped on the Dock icon. Holding
    /// Option while dropping archives extracts them instead of opening them.
    func application(_ application: NSApplication, open urls: [URL]) {
        if NSEvent.modifierFlags.contains(.option), urls.allSatisfy(AppRouter.isArchive) {
            AppRouter.shared.extractHere(urls)
        } else {
            AppRouter.shared.open(urls)
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
