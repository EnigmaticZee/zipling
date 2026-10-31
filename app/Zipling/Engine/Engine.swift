import ArcaEngine
import Foundation
import os

/// An error reported by the archive engine.
struct EngineError: LocalizedError {
    let message: String
    /// The archive was read but is damaged or cut short (not just unknown).
    var damaged = false
    var errorDescription: String? { message }
    var isCancellation: Bool { message == "cancelled" }
    /// The engine doesn't handle this archive (see `Engine.extractZip`); another reader should.
    var isUnsupported: Bool { message.hasPrefix("unsupported:") }
}

/// Cancellation handle for one engine operation.
final class EngineTask: @unchecked Sendable {
    // The engine's ArcaTask is internally synchronised; the pointer is never mutated.
    fileprivate let handle: OpaquePointer

    init() { handle = arca_task_new() }
    func cancel() { arca_task_cancel(handle) }
    deinit { arca_task_free(handle) }
}

/// Forwards engine progress (called from engine threads) at most ~20 times a second.
final class ProgressRelay: Sendable {
    private let handler: @Sendable (UInt64, UInt64) -> Void
    private let last = OSAllocatedUnfairLock(initialState: ContinuousClock.now - .seconds(1))

    init(_ handler: @escaping @Sendable (UInt64, UInt64) -> Void) {
        self.handler = handler
    }

    func report(done: UInt64, total: UInt64) {
        let now = ContinuousClock.now
        let due = last.withLock { last -> Bool in
            guard now - last >= .milliseconds(50) || done >= total else { return false }
            last = now
            return true
        }
        if due { handler(done, total) }
    }
}

enum Level: String, CaseIterable, Identifiable, Codable, Sendable {
    case fast, balanced, max, ultra
    var id: String { rawValue }
}

/// Swift face of the Rust engine (`engine/arca-ffi`). Every call runs off the main
/// thread; the engine itself spreads work across all cores.
enum Engine {
    static let version = String(cString: arca_version())

    static func list(_ archive: URL) async throws -> Listing {
        let path = archive.path
        return try await detached {
            Timing.mark("engine.list: calling")
            let raw = arca_list(path)
            Timing.mark("engine.list: returned")
            let listing: Listing = try decode(raw)
            Timing.mark("engine.list: decoded")
            return listing
        }
    }

    static func pack(
        _ inputs: [URL], into output: URL, level: Level, task: EngineTask,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> PackResult {
        let request = try json(PackRequest(inputs: inputs.map(\.path), output: output.path, level: level.rawValue))
        return try await withProgress(progress) { ctx in
            Timing.mark("engine.pack: calling")
            let raw = arca_pack(request, task.handle, trampoline, ctx)
            Timing.mark("engine.pack: returned")
            let result: PackResult = try decode(raw)
            Timing.mark("engine.pack: decoded")
            return result
        }
    }

    /// With `quarantine`, the engine sets it on every item as it writes them.
    static func extract(
        _ archive: URL, to dest: URL, only: [String], quarantine: Data? = nil, task: EngineTask,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ExtractResult {
        let request = try json(ExtractRequest(archive: archive.path, dest: dest.path, only: only, overwrite: false,
                                              quarantine: quarantine.map { [UInt8]($0) }))
        var result: ExtractResult = try await withProgress(progress) { ctx in
            Timing.mark("engine.extract: calling")
            let raw = arca_extract(request, task.handle, trampoline, ctx)
            Timing.mark("engine.extract: returned")
            let result: ExtractResult = try decode(raw)
            Timing.mark("engine.extract: decoded")
            return result
        }
        result.quarantined = quarantine != nil
        return result
    }

    /// Extracts a ZIP archive with the engine's own reader: every core, every CRC
    /// checked, Mac metadata from `__MACOSX` applied in memory, links that lead out
    /// left out. Throws an error whose `isUnsupported` is true for archives it doesn't
    /// handle (encryption, other methods, legacy name encodings, split archives).
    static func extractZip(
        _ archive: URL, to dest: URL, only: [String], quarantine: Data? = nil, task: EngineTask,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ExtractResult {
        let request = try json(ZipExtractRequest(archive: archive.path, dest: dest.path, only: only, overwrite: false,
                                                 quarantine: quarantine.map { [UInt8]($0) }))
        var result: ExtractResult = try await withProgress(progress) { ctx in
            try decode(arca_zip_extract(request, task.handle, trampoline, ctx))
        }
        result.quarantined = quarantine != nil
        return result
    }

    /// Makes a ZIP archive with the engine's own writer: every core (large files in
    /// pieces), Deflate at `level`, UTF-8 names, permissions, times, links as links.
    /// No passwords or split volumes: 7-Zip makes those.
    static func zip(
        _ inputs: [URL], into output: URL, level: Int, leaveOutMacFiles: Bool, task: EngineTask,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ZipCreateResult {
        let request = try json(ZipCreateRequest(inputs: inputs.map(\.path), output: output.path, level: level,
                                                leaveOutMacFiles: leaveOutMacFiles))
        return try await withProgress(progress) { ctx in
            try decode(arca_zip_create(request, task.handle, trampoline, ctx))
        }
    }

    /// Checks every entry of a ZIP archive (its CRC-32s), writing nothing. Throws an
    /// error whose `isUnsupported` is true for archives the engine doesn't read.
    static func verifyZip(_ archive: URL, task: EngineTask) async throws {
        let path = archive.path
        let _: ZipVerifyResult = try await withProgress({ _, _ in }) { ctx in
            try decode(arca_zip_verify(path, task.handle, trampoline, ctx))
        }
    }

    static func verify(
        _ archive: URL, task: EngineTask, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ExtractResult {
        let path = archive.path
        return try await withProgress(progress) { ctx in
            try decode(arca_verify(path, task.handle, trampoline, ctx))
        }
    }

    // MARK: - Plumbing

    private static let trampoline: ArcaProgress = { ctx, done, total in
        guard let ctx else { return }
        Unmanaged<ProgressRelay>.fromOpaque(ctx).takeUnretainedValue().report(done: done, total: total)
    }

    private static func withProgress<T: Sendable>(
        _ progress: @escaping @Sendable (UInt64, UInt64) -> Void,
        _ body: @escaping @Sendable (UnsafeMutableRawPointer) throws -> T
    ) async throws -> T {
        let relay = ProgressRelay(progress)
        return try await detached {
            // The relay outlives the call: the engine only reports while the call runs.
            try withExtendedLifetime(relay) {
                try body(Unmanaged.passUnretained(relay).toOpaque())
            }
        }
    }

    private static func detached<T: Sendable>(_ work: @escaping @Sendable () throws -> T) async throws -> T {
        try await Task.detached(priority: .userInitiated, operation: work).value
    }

    private struct Envelope<T: Decodable>: Decodable {
        let ok: T?
        let error: String?
    }

    private static func decode<T: Decodable>(_ raw: UnsafeMutablePointer<CChar>?) throws -> T {
        guard let raw else { throw EngineError(message: "The archive engine returned nothing.") }
        defer { arca_string_free(raw) }
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        let envelope = try decoder.decode(Envelope<T>.self, from: Data(bytes: raw, count: strlen(raw)))
        if let error = envelope.error { throw EngineError(message: error) }
        guard let ok = envelope.ok else { throw EngineError(message: "The archive engine sent an empty reply.") }
        return ok
    }

    private static func json<T: Encodable>(_ value: T) throws -> String {
        String(decoding: try JSONEncoder().encode(value), as: UTF8.self)
    }

    private struct PackRequest: Encodable {
        let inputs: [String]
        let output: String
        let level: String
    }

    private struct ExtractRequest: Encodable {
        let archive: String
        let dest: String
        let only: [String]
        let overwrite: Bool
        let quarantine: [UInt8]?
    }

    private struct ZipCreateRequest: Encodable {
        let inputs: [String]
        let output: String
        let level: Int
        let leaveOutMacFiles: Bool

        enum CodingKeys: String, CodingKey {
            case inputs, output, level
            case leaveOutMacFiles = "leave_out_mac_files"
        }
    }

    private struct ZipVerifyResult: Decodable, Sendable {
        let entries: UInt64
        let bytes: UInt64
        let seconds: Double
    }

    private struct ZipExtractRequest: Encodable {
        let archive: String
        let dest: String
        let only: [String]
        let overwrite: Bool
        let quarantine: [UInt8]?
    }
}

/// Timing marks around the engine, for finding where the app spends time that the
/// engine does not. Off unless `defaults write com.observantsystems.zipling timing
/// -bool true`; then `log show --last 5m --style compact --predicate 'subsystem ==
/// "com.observantsystems.zipling" AND category == "timing"'`.
enum Timing {
    nonisolated(unsafe) private static var start = ContinuousClock.now
    private static let on = UserDefaults.standard.bool(forKey: "timing")
    private static let log = Logger(subsystem: "com.observantsystems.zipling", category: "timing")

    static func begin(_ what: String) {
        guard on else { return }
        start = .now
        log.notice("\(what, privacy: .public): start")
    }

    static func mark(_ what: String) {
        guard on else { return }
        let ms = (ContinuousClock.now - start) / .milliseconds(1)
        log.notice("\(what, privacy: .public): \(ms, format: .fixed(precision: 1), privacy: .public) ms")
    }
}
