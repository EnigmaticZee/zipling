import AppKit
import Observation

/// Every extraction, compression and check in the app, wherever it was started, so
/// they can run in the background with their progress on the Dock icon.
@MainActor
@Observable
final class JobCenter {
    static let shared = JobCenter()

    private(set) var jobs: [Job] = []
    @ObservationIgnored private var timer: Timer?
    @ObservationIgnored private lazy var dock = DockProgress()

    var running: [Job] { jobs.filter(\.isRunning) }

    func track(_ job: Job) {
        jobs.insert(job, at: 0)
        // Keep the last few finished ones for the activity list.
        var finished = 0
        jobs.removeAll { job in
            guard !job.isRunning else { return false }
            finished += 1
            return finished > 12
        }
        guard timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { _ in
            Task { @MainActor in JobCenter.shared.tick() }
        }
        tick()
    }

    func dismiss(_ job: Job) {
        jobs.removeAll { $0 === job }
    }

    func clearFinished() {
        jobs.removeAll { !$0.isRunning }
    }

    /// Called by a job when it ends: draws attention if Zipling is in the background.
    func finished(_ job: Job) {
        tick()
        if !NSApp.isActive { NSApp.requestUserAttention(.informationalRequest) }
    }

    private func tick() {
        let running = running
        guard !running.isEmpty else {
            timer?.invalidate()
            timer = nil
            dock.hide()
            return
        }
        let done = running.reduce(0) { $0 + $1.done }, total = running.reduce(0) { $0 + $1.total }
        dock.show(fraction: total == 0 ? 0 : Double(done) / Double(total), count: running.count)
    }
}

/// The Dock icon with a progress bar under it while jobs run.
@MainActor
private final class DockProgress {
    private let view: NSView
    private let bar: NSProgressIndicator

    init() {
        let size = NSApp.dockTile.size
        view = NSImageView(image: NSApp.applicationIconImage)
        view.frame = NSRect(origin: .zero, size: size)
        bar = NSProgressIndicator(frame: NSRect(x: size.width * 0.1, y: size.height * 0.06,
                                                width: size.width * 0.8, height: size.height * 0.12))
        bar.style = .bar
        bar.isIndeterminate = false
        bar.minValue = 0
        bar.maxValue = 1
        view.addSubview(bar)
    }

    func show(fraction: Double, count: Int) {
        let tile = NSApp.dockTile
        if tile.contentView !== view { tile.contentView = view }
        bar.doubleValue = fraction
        tile.badgeLabel = count > 1 ? "\(count)" : nil
        tile.display()
    }

    func hide() {
        let tile = NSApp.dockTile
        guard tile.contentView != nil || tile.badgeLabel != nil else { return }
        tile.contentView = nil
        tile.badgeLabel = nil
        tile.display()
    }
}
