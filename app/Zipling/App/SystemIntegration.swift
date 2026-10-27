import AppKit

/// Finder's right-click menu (Services and Quick Actions): Compress with Zipling,
/// Extract Here with Zipling and Open in Zipling. Declared under NSServices in
/// Info.plist; macOS calls these with the selected files on a pasteboard.
final class ServiceProvider: NSObject {
    @objc func compressFiles(_ pasteboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString>) {
        let urls = Self.files(pasteboard)
        Task { @MainActor in AppRouter.shared.compress(urls) }
    }

    @objc func extractFiles(_ pasteboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString>) {
        let urls = Self.files(pasteboard)
        Task { @MainActor in AppRouter.shared.extractHere(urls) }
    }

    @objc func openFiles(_ pasteboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString>) {
        let urls = Self.files(pasteboard)
        Task { @MainActor in AppRouter.shared.open(urls) }
    }

    private static func files(_ pasteboard: NSPasteboard) -> [URL] {
        (pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL]) ?? []
    }
}

/// The menu-bar icon: drop files on it to compress them, or archives to extract
/// them; click it for the same actions and what's running.
@MainActor
final class MenuBarDropZone: NSObject, NSMenuDelegate, NSWindowDelegate {
    static let shared = MenuBarDropZone()
    private var item: NSStatusItem?
    private var progressTimer: Timer?

    func update() {
        if Preferences.showMenuBarIcon { show() } else { hide() }
    }

    private func show() {
        guard item == nil else { return }
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        guard let button = item.button else { return }
        button.image = NSImage(systemSymbolName: "shippingbox", accessibilityDescription: "Zipling")
        button.toolTip = "Zipling: drop files to compress, or archives to extract"
        // The status item's window passes drags on to its delegate.
        button.window?.registerForDraggedTypes([.fileURL])
        button.window?.delegate = self
        let menu = NSMenu()
        menu.delegate = self
        item.menu = menu
        self.item = item
        progressTimer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { _ in
            Task { @MainActor in MenuBarDropZone.shared.showProgress() }
        }
    }

    private func hide() {
        progressTimer?.invalidate()
        progressTimer = nil
        if let item { NSStatusBar.system.removeStatusItem(item) }
        item = nil
    }

    /// While something runs, the icon shows how far it's got.
    private func showProgress() {
        guard let button = item?.button else { return }
        let running = JobCenter.shared.running
        let total = running.reduce(0) { $0 + $1.total }, done = running.reduce(0) { $0 + $1.done }
        let title = running.isEmpty || total == 0 ? "" : " " + Format.percent(Double(done) / Double(total))
        if button.title != title { button.title = title }
    }

    func menuNeedsUpdate(_ menu: NSMenu) {
        menu.removeAllItems()
        func add(_ title: String, _ action: Selector, key: String = "") {
            let entry = NSMenuItem(title: title, action: action, keyEquivalent: key)
            entry.target = self
            menu.addItem(entry)
        }
        let running = JobCenter.shared.running
        if !running.isEmpty {
            for job in running.prefix(5) {
                let entry = NSMenuItem(title: "\(job.title) \(Format.percent(job.fraction))", action: nil, keyEquivalent: "")
                entry.isEnabled = false
                menu.addItem(entry)
            }
            menu.addItem(.separator())
        }
        add("Compress Files…", #selector(compress))
        add("Extract Archives…", #selector(extract))
        add("Open Archive…", #selector(openArchive))
        menu.addItem(.separator())
        add("Show Zipling", #selector(showApp))
        add("Hide This Icon", #selector(hideIcon))
    }

    // MARK: Drops

    @objc func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        item?.button?.highlight(true)
        return .copy
    }

    @objc func draggingExited(_ sender: NSDraggingInfo?) {
        item?.button?.highlight(false)
    }

    @objc func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        item?.button?.highlight(false)
        let urls = sender.draggingPasteboard.readObjects(forClasses: [NSURL.self],
                                                         options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        guard !urls.isEmpty else { return false }
        if Self.extracts(urls) { AppRouter.shared.extractHere(urls) } else { AppRouter.shared.compress(urls) }
        return true
    }

    /// Dropped archives are extracted; anything else (or a mix) is compressed.
    static func extracts(_ urls: [URL]) -> Bool {
        !urls.isEmpty && urls.allSatisfy(AppRouter.isArchive)
    }

    // MARK: Menu actions

    @objc private func compress() { activate { AppRouter.shared.chooseFilesToCompress() } }
    @objc private func extract() { activate { AppRouter.shared.chooseArchivesToExtract() } }
    @objc private func openArchive() { activate { AppRouter.shared.chooseArchive() } }
    @objc private func showApp() { activate { AppRouter.shared.showHome() } }

    @objc private func hideIcon() {
        UserDefaults.standard.set(false, forKey: Preferences.Key.showMenuBarIcon)
        update()
    }

    private func activate(_ then: () -> Void) {
        NSApp.activate()
        then()
    }
}
