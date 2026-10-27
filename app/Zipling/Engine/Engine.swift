import ArcaEngine
import Foundation
import os

/// An error reported by the archive engine.
struct EngineError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
    var isCancellation: Bool { message == "cancelled" }
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
        return try await detached { try decode(arca_list(path)) }
    }

    static func pack(
        _ inputs: [URL], into output: URL, level: Level, task: EngineTask,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> PackResult {
        let request = try json(PackRequest(inputs: inputs.map(\.path), output: output.path, level: level.rawValue))
        return try await withProgress(progress) { ctx in
            try decode(arca_pack(request, task.handle, trampoline, ctx))
        }
    }

    static func extract(
        _ archive: URL, to dest: URL, only: [String], task: EngineTask,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ExtractResult {
        let request = try json(ExtractRequest(archive: archive.path, dest: dest.path, only: only, overwrite: false))
        return try await withProgress(progress) { ctx in
            try decode(arca_extract(request, task.handle, trampoline, ctx))
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
    }
}
