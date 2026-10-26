import Foundation
import os
import Testing
@testable import Arca

/// End-to-end tests through the real Rust engine: the C bridge, JSON, progress,
/// cancellation, and the browser model's tree building and file placement.
@MainActor
struct ArcaTests {
    /// Project/docs/readme.txt, Project/docs/nested/copy.txt (a duplicate), Project/noise.bin
    private func makeTree() throws -> URL {
        let root = FileManager.default.temporaryDirectory.appending(path: "ArcaTests-\(UUID().uuidString)/Project")
        try FileManager.default.createDirectory(at: root.appending(path: "docs/nested"), withIntermediateDirectories: true)
        let text = Data(String(repeating: "hello arca\n", count: 5000).utf8)
        try text.write(to: root.appending(path: "docs/readme.txt"))
        try text.write(to: root.appending(path: "docs/nested/copy.txt"))
        var rng = SystemRandomNumberGenerator()
        try Data((0..<300_000).map { _ in UInt8.random(in: 0...255, using: &rng) }).write(to: root.appending(path: "noise.bin"))
        return root
    }

    private func pack(_ src: URL) async throws -> (URL, PackResult) {
        let archive = src.deletingLastPathComponent().appending(path: "Project.arca")
        let result = try await Engine.pack([src], into: archive, level: .balanced, task: EngineTask()) { _, _ in }
        return (archive, result)
    }

    @Test func packListVerifyExtract() async throws {
        let src = try makeTree()
        let archive = src.deletingLastPathComponent().appending(path: "Project.arca")
        let progress = ProgressLog()
        let result = try await Engine.pack([src], into: archive, level: .balanced, task: EngineTask()) { done, total in
            progress.record(done, total)
        }
        #expect(result.files == 3)
        #expect(result.inputBytes == 410_000)
        #expect(result.dupFileBytes == 55_000, "the copy should be stored once")
        #expect(result.outputBytes < result.inputBytes)
        #expect(progress.last == [410_000, 410_000], "progress must end at 100%")

        let listing = try await Engine.list(archive)
        #expect(listing.files == 3)
        #expect(listing.entries.contains { $0.path == "Project/docs/nested/copy.txt" && $0.size == 55_000 })

        let verified = try await Engine.verify(archive, task: EngineTask()) { _, _ in }
        #expect(verified.files == 3)

        let dest = src.deletingLastPathComponent().appending(path: "out")
        _ = try await Engine.extract(archive, to: dest, only: ["Project/docs"], task: EngineTask()) { _, _ in }
        let fm = FileManager.default
        #expect(fm.contentsEqual(atPath: dest.appending(path: "Project/docs/readme.txt").path,
                                 andPath: src.appending(path: "docs/readme.txt").path))
        #expect(!fm.fileExists(atPath: dest.appending(path: "Project/noise.bin").path), "only the selected folder")
    }

    @Test func cancelledPackLeavesNothingBehind() async throws {
        let src = try makeTree()
        let archive = src.deletingLastPathComponent().appending(path: "Cancelled.arca")
        let task = EngineTask()
        task.cancel()
        await #expect {
            _ = try await Engine.pack([src], into: archive, level: .balanced, task: task) { _, _ in }
        } throws: { ($0 as? EngineError)?.isCancellation == true }
        let leftovers = try FileManager.default.contentsOfDirectory(atPath: src.deletingLastPathComponent().path)
        #expect(leftovers == ["Project"], "no archive and no .partial file")
    }

    @Test func engineErrorsBecomeMessages() async throws {
        await #expect {
            _ = try await Engine.list(URL(filePath: "/nonexistent/missing.arca"))
        } throws: { ($0 as? EngineError)?.message.contains("missing.arca") == true }
    }

    @Test func browserTreeAndFinderStylePlacement() async throws {
        let src = try makeTree()
        let (archive, _) = try await pack(src)
        let model = ArchiveModel(url: archive)
        await model.load()

        #expect(model.visibleItems.map(\.name) == ["Project"])
        let project = try #require(model.visibleItems.first)
        #expect(project.isFolder)
        #expect(project.size == 410_000, "folders total their contents")
        model.enter(project)
        #expect(model.visibleItems.map(\.name) == ["docs", "noise.bin"], "folders first")
        #expect(model.crumbs.map(\.name) == ["Project"])

        model.search = "copy"
        #expect(model.visibleItems.map(\.id) == ["Project/docs/nested/copy.txt"])
        model.search = ""

        // Extracting the same folder twice must not overwrite: the second is "docs 2".
        let dest = src.deletingLastPathComponent().appending(path: "dest")
        try FileManager.default.createDirectory(at: dest, withIntermediateDirectories: true)
        for _ in 0..<2 {
            model.extract(["Project/docs", "Project/docs/readme.txt"], to: dest, then: .nothing)
            try await waitForJob(model)
        }
        let names = try FileManager.default.contentsOfDirectory(atPath: dest.path).sorted()
        #expect(names == ["docs", "docs 2"], "selected inner items fold into their folder; staging is cleaned up")
        #expect(FileManager.default.fileExists(atPath: dest.appending(path: "docs 2/nested/copy.txt").path))
    }

    private func waitForJob(_ model: ArchiveModel) async throws {
        for _ in 0..<1000 {
            if let job = model.job, !job.isRunning {
                if case .failed(let message) = job.phase { Issue.record("job failed: \(message)") }
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("job did not finish")
    }
}

/// Collects progress callbacks, which arrive on engine threads.
private final class ProgressLog: Sendable {
    private let state = OSAllocatedUnfairLock(initialState: [UInt64]())

    func record(_ done: UInt64, _ total: UInt64) {
        state.withLock { $0 = [done, total] }
    }

    var last: [UInt64] { state.withLock { $0 } }
}
