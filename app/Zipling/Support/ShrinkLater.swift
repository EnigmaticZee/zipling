import AppKit
import IOKit.ps

/// Compress fast now, shrink later: a Zipling archive made at Fast or Balanced is
/// re-packed at Max while the Mac is idle and plugged in. The whole archive is made
/// again next to the original and swapped in only once it's complete, checked, holds
/// the same files and is smaller. The files inside never change; only the size drops.
@MainActor
@Observable
final class ShrinkLater {
    static let shared = ShrinkLater()

    /// An archive waiting, as it was when it was made: if it's changed or moved since,
    /// it's left alone.
    struct Entry: Codable, Equatable {
        let path: String
        let size: UInt64
        let modified: Double
    }

    private(set) var queue: [Entry] = []
    private(set) var running = false
    @ObservationIgnored private let key = "shrinkLaterQueue"
    @ObservationIgnored private var activity: NSBackgroundActivityScheduler?
    @ObservationIgnored var defaults = UserDefaults.standard {
        didSet { queue = load() }
    }

    init() {
        queue = load()
    }

    func add(_ url: URL) {
        guard let entry = Self.entry(for: url) else { return }
        queue.removeAll { $0.path == entry.path }
        queue.append(entry)
        save()
        schedule()
    }

    /// Asks macOS to run the queue when the Mac isn't busy.
    func schedule() {
        guard activity == nil, !queue.isEmpty else { return }
        let activity = NSBackgroundActivityScheduler(identifier: "com.observantsystems.zipling.shrink-later")
        activity.repeats = true
        activity.interval = 15 * 60
        activity.tolerance = 5 * 60
        activity.qualityOfService = .background
        activity.schedule { completion in
            Task { @MainActor in completion(await ShrinkLater.shared.runOnce()) }
        }
        self.activity = activity
    }

    /// One archive per turn, and only on mains power.
    func runOnce() async -> NSBackgroundActivityScheduler.Result {
        guard !running, Self.onMainsPower else { return .deferred }
        guard let entry = queue.first else {
            stop()
            return .finished
        }
        await shrink(entry)
        if queue.isEmpty { stop() }
        return .finished
    }

    /// Everything waiting, now, whatever the power source (Settings' Shrink Now).
    func shrinkAll() async {
        while let entry = queue.first, !running {
            await shrink(entry)
        }
        if queue.isEmpty { stop() }
    }

    private func shrink(_ entry: Entry) async {
        running = true
        defer { running = false }
        let url = URL(filePath: entry.path)
        queue.removeAll { $0 == entry }
        save()
        guard Self.entry(for: url) == entry else { return }  // moved, changed or deleted since
        let job = Job(title: "Shrinking “\(url.lastPathComponent)”…")
        JobCenter.shared.track(job)
        do {
            if let (before, after) = try await Self.shrink(url, task: job.task, cancel: job.cancelToken,
                                                          progress: job.progressHandler) {
                job.finish(nil, outcome: "Re-packed at Max: \(Format.bytes(before)) → \(Format.bytes(after)), same files.",
                           placed: [url])
            } else {
                job.finish(nil, outcome: "Already as small as Max makes it, so it was left as it was.")
            }
        } catch {
            job.finish(error)
        }
    }

    /// Re-packs one Zipling archive at Max. Returns the sizes before and after, or nil
    /// when Max wasn't smaller and the archive was left alone.
    static func shrink(
        _ url: URL, task: EngineTask, cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> (UInt64, UInt64)? {
        let fm = FileManager.default
        guard Archives.isZipling(url) else { throw EngineError(message: "“\(url.lastPathComponent)” isn't a Zipling archive.") }
        let work = url.deletingLastPathComponent().appending(path: ".zipling-shrink-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: work) }
        let original = try await Engine.list(url)
        let opened = OpenedArchive(source: url, backend: .arca, listing: original)
        let contents = work.appending(path: "contents")
        _ = try await Archives.extract(opened, to: contents, only: [], task: task, cancel: cancel) { done, total in
            progress(done / 3, total)
        }
        let items = SafeExtraction.names(in: contents.path).sorted().map { contents.appending(path: $0) }
        let made = try await Creation.create(
            items, into: work.appending(path: url.lastPathComponent), options: CreateOptions(format: .zlg, level: .max),
            password: nil, task: task, cancel: cancel) { done, total in
            progress(total / 3 + done / 3, total)
        }
        guard let rebuilt = made.files.first else { return nil }
        // The same files, every block checked.
        let listing = try await Engine.list(rebuilt)
        guard listing.files == original.files, listing.totalSize == original.totalSize,
              Set(listing.entries.map(\.path)) == Set(original.entries.map(\.path)) else {
            throw EngineError(message: "The re-packed archive didn't match the original, so the original was kept.")
        }
        _ = try await Engine.verify(rebuilt, task: task) { done, total in progress(2 * total / 3 + done / 3, total) }
        let before = Conversion.size(of: url), after = Conversion.size(of: rebuilt)
        guard after < before else { return nil }
        _ = try fm.replaceItemAt(url, withItemAt: rebuilt)
        // The recovery file described the old bytes.
        try? await Recovery.refresh(for: url, cancel: cancel)
        return (before, after)
    }

    private func stop() {
        activity?.invalidate()
        activity = nil
    }

    private func load() -> [Entry] {
        guard let data = defaults.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([Entry].self, from: data)) ?? []
    }

    private func save() {
        defaults.set(try? JSONEncoder().encode(queue), forKey: key)
    }

    private static func entry(for url: URL) -> Entry? {
        guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = (attributes[.size] as? NSNumber)?.uint64Value,
              let modified = attributes[.modificationDate] as? Date else { return nil }
        return Entry(path: url.path, size: size, modified: modified.timeIntervalSince1970)
    }

    /// Plugged in (or a Mac without a battery).
    static var onMainsPower: Bool {
        guard let info = IOPSCopyPowerSourcesInfo()?.takeRetainedValue(),
              let type = IOPSGetProvidingPowerSourceType(info)?.takeUnretainedValue() else { return true }
        return (type as String) == kIOPMACPowerKey
    }
}
