import Foundation
import Observation

/// One long-running engine operation, observed by the UI.
@MainActor
@Observable
final class Job: Identifiable {
    enum Phase {
        case running
        case finished
        case failed(String)
        case cancelled
    }

    let id = UUID()
    let title: String
    private(set) var phase: Phase = .running
    /// What happened, in a sentence, once it's finished.
    private(set) var outcome: String?
    /// What it made, for Show in Finder.
    private(set) var placed: [URL] = []
    private(set) var done: UInt64 = 0
    private(set) var total: UInt64 = 0
    private let started = Date()
    @ObservationIgnored let task = EngineTask()
    /// Stops helper processes (7-Zip); `task` stops the Arca engine.
    @ObservationIgnored let cancelToken = CancelToken()

    init(title: String) {
        self.title = title
    }

    /// The title once it's over: "Extracted “Photos.zip”" for "Extracting “Photos.zip”…".
    var finishedTitle: String {
        guard case .finished = phase else { return title }
        let verbs = ["Extracting": "Extracted", "Compressing": "Compressed", "Verifying": "Verified",
                     "Adding": "Added", "Updating": "Updated", "Deleting": "Deleted", "Renaming": "Renamed"]
        var text = title.hasSuffix("…") ? String(title.dropLast()) : title
        for (doing, done) in verbs where text.hasPrefix(doing) {
            text = done + text.dropFirst(doing.count)
            break
        }
        return text
    }

    var fraction: Double { total == 0 ? 0 : min(1, Double(done) / Double(total)) }
    var elapsed: TimeInterval { Date().timeIntervalSince(started) }
    var isRunning: Bool { if case .running = phase { true } else { false } }

    var detail: String {
        guard total > 0 else { return "Preparing…" }
        let rate = elapsed > 0.3 ? Double(done) / elapsed : 0
        var parts = ["\(Format.bytes(done)) of \(Format.bytes(total))"]
        if rate > 0 {
            parts.append(Format.bytes(UInt64(rate)) + "/s")
            if done < total {
                parts.append("about \(Format.duration(Double(total - done) / rate)) left")
            }
        }
        return parts.joined(separator: " · ")
    }

    /// A progress callback safe to hand to the engine.
    var progressHandler: @Sendable (UInt64, UInt64) -> Void {
        { [weak self] done, total in
            Task { @MainActor in self?.update(done: done, total: total) }
        }
    }

    func update(done: UInt64, total: UInt64) {
        self.done = done
        self.total = total
    }

    func cancel() {
        task.cancel()
        cancelToken.cancel()
    }

    func finish(_ error: Error?, outcome: String? = nil, placed: [URL] = []) {
        self.outcome = outcome
        self.placed = placed
        switch error {
        case nil: phase = .finished
        case let e as EngineError where e.isCancellation: phase = .cancelled
        case let e?: phase = .failed(e.localizedDescription)
        }
        JobCenter.shared.finished(self)
    }
}
