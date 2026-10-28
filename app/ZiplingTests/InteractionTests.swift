import Foundation
import Testing
import UniformTypeIdentifiers
@testable import Zipling

/// What happens behind clicks and drags: dragging out, Quick Look following the
/// selection, editing a file in its own app and putting it back, batch Extract Here,
/// menu-bar drops and saved presets.
@MainActor
struct InteractionTests {
    private let fm = FileManager.default
    private let readme = String(repeating: "hello again!\n", count: 1500)

    private func makeArchive(_ name: String = "Project.zip") throws -> (URL, URL) {
        let root = fm.temporaryDirectory.appending(path: "InteractionTests-\(UUID().uuidString)")
        let project = root.appending(path: "Project")
        try fm.createDirectory(at: project.appending(path: "docs/nested"), withIntermediateDirectories: true)
        try readme.write(to: project.appending(path: "docs/readme.txt"), atomically: true, encoding: .utf8)
        try "copy".write(to: project.appending(path: "docs/nested/copy.txt"), atomically: true, encoding: .utf8)
        let archive = root.appending(path: name)
        let p = Process()
        p.executableURL = SevenZip.executable
        p.arguments = ["a", name.hasSuffix(".zip") ? "-tzip" : "-t7z", archive.path, "Project"]
        p.currentDirectoryURL = root
        p.standardOutput = FileHandle.nullDevice
        try p.run()
        p.waitUntilExit()
        return (root, archive)
    }

    private func until(_ condition: @MainActor () -> Bool) async throws {
        for _ in 0..<1000 where !condition() { try await Task.sleep(for: .milliseconds(10)) }
    }

    /// What a drop target (Finder) receives: the file the drag exports.
    private func dropped(_ drag: ArchiveDrag, as type: UTType) async throws -> (name: String, contents: [String]) {
        let provider = NSItemProvider()
        provider.register(drag)
        #expect(provider.registeredContentTypes.contains { $0.conforms(to: type) }, "offered as \(type.identifier)")
        return try await withCheckedThrowingContinuation { continuation in
            _ = provider.loadFileRepresentation(for: type) { url, _, error in
                guard let url else { return continuation.resume(throwing: error ?? CocoaError(.fileNoSuchFile)) }
                // The file only exists while this handler runs, as for a real drop.
                let isFolder = (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
                let contents = isFolder
                    ? (FileManager.default.subpaths(atPath: url.path) ?? []).sorted()
                    : [(try? String(contentsOf: url, encoding: .utf8)) ?? ""]
                continuation.resume(returning: (url.lastPathComponent, contents))
            }
        }
    }

    @Test func draggingOutGivesTheRealFiles() async throws {
        let (_, archive) = try makeArchive()
        let model = ArchiveModel(url: archive)
        await model.load()
        let file = try #require(model.item("Project/docs/readme.txt"))
        let text = try await dropped(ArchiveDrag(item: file, model: model), as: .data)
        #expect(text.name == "readme.txt")
        #expect(text.contents == [readme], "the file comes out of the archive on drop")

        let folder = try #require(model.item("Project/docs"))
        let tree = try await dropped(ArchiveDrag(item: folder, model: model), as: .folder)
        #expect(tree.name == "docs")
        #expect(tree.contents == ["nested", "nested/copy.txt", "readme.txt"], "a folder comes out whole")
        model.close()
    }

    @Test func quickLookFollowsTheSelection() async throws {
        let (_, archive) = try makeArchive("Project.7z")
        let model = ArchiveModel(url: archive)
        await model.load()
        model.show(folder: "Project/docs")
        model.selection = ["Project/docs/readme.txt"]
        model.toggleQuickLook()
        try await until { model.quickLookURL != nil }
        #expect(model.quickLookURL?.lastPathComponent == "readme.txt")
        #expect(try String(contentsOf: #require(model.quickLookURL), encoding: .utf8) == readme)

        // Like Finder: the open panel shows whatever gets selected next.
        model.show(folder: "Project/docs/nested")
        model.selection = ["Project/docs/nested/copy.txt"]
        try await until { model.quickLookURL?.lastPathComponent == "copy.txt" }
        #expect(model.quickLookURL?.lastPathComponent == "copy.txt")
        model.toggleQuickLook()
        #expect(model.quickLookURL == nil, "space again closes it")
        model.close()
    }

    @Test func editingInItsOwnAppPutsItBack() async throws {
        let (root, archive) = try makeArchive()
        let model = ArchiveModel(url: archive)
        await model.load()
        let file = try #require(model.item("Project/docs/readme.txt"))
        var opened: URL?
        model.editInApp(file) { opened = $0 }
        try await until { opened != nil }
        let copy = try #require(opened)
        #expect(copy.lastPathComponent == "readme.txt")

        model.checkEdits()
        #expect(model.changedOutside.isEmpty, "nothing to put back before a save")
        // Another app saves it (a second later, as far as the file system can tell).
        try "edited elsewhere".write(to: copy, atomically: true, encoding: .utf8)
        try fm.setAttributes([.modificationDate: Date().addingTimeInterval(2)], ofItemAtPath: copy.path)
        model.checkEdits()
        let changed = try #require(model.changedOutside.first)
        #expect(changed.folder == "Project/docs")

        model.saveBack(changed)
        #expect(model.changedOutside.isEmpty)
        try await until { model.job.map { !$0.isRunning } ?? false }
        if case .failed(let message) = model.job?.phase { Issue.record("save back failed: \(message)") }

        let reopened = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        let out = root.appending(path: "out")
        _ = try await Archives.extract(reopened, to: out, only: ["Project/docs/readme.txt"], task: EngineTask(),
                                       cancel: CancelToken()) { _, _ in }
        #expect(try String(contentsOf: out.appending(path: "Project/docs/readme.txt"), encoding: .utf8) == "edited elsewhere")
        #expect(model.folder == "" || model.levels.first?.byID["Project/docs/readme.txt"] != nil, "still browsable")
        model.close()
    }

    @Test func batchExtractHere() async throws {
        let (root, zip) = try makeArchive()
        let (_, sevenZip) = try makeArchive("Second.7z")
        let second = root.appending(path: "Second.7z")
        try fm.moveItem(at: sevenZip, to: second)

        await BatchExtraction.run([zip, second], conflicts: Conflicts(policy: .keepBoth), reveal: false)
        // Both hold one "Project" folder, so the second one gets a number, as in Finder.
        #expect(fm.fileExists(atPath: root.appending(path: "Project 2/docs/readme.txt").path))
        #expect(fm.fileExists(atPath: root.appending(path: "Project 3/docs/readme.txt").path))
        let finished = JobCenter.shared.jobs.prefix(2)
        #expect(finished.allSatisfy { if case .finished = $0.phase { true } else { false } })
        #expect(finished.map(\.finishedTitle).sorted() == ["Extracted “Project.zip”", "Extracted “Second.7z”"])
    }

    @Test func menuBarDropsExtractArchivesAndCompressTheRest() throws {
        let (root, zip) = try makeArchive()
        let folder = root.appending(path: "Project")
        #expect(MenuBarDropZone.extracts([zip]))
        #expect(!MenuBarDropZone.extracts([folder]))
        #expect(!MenuBarDropZone.extracts([zip, folder]), "a mix is compressed together")
        #expect(!MenuBarDropZone.extracts([]))
    }

    @Test func dropsOpenArchivesOnTheirOwnAndCompressAMix() throws {
        let (root, zip) = try makeArchive()
        let (_, sevenZip) = try makeArchive("Other.7z")
        let folder = root.appending(path: "Project")
        let text = root.appending(path: "Project/docs/readme.txt")
        #expect(AppRouter.opens([zip]))
        #expect(AppRouter.opens([zip, sevenZip]), "archives on their own each open")
        #expect(!AppRouter.opens([text]) && !AppRouter.opens([folder]))
        #expect(!AppRouter.opens([text, zip]), "a file and an archive are compressed together, not split")
        #expect(!AppRouter.opens([]))
    }

    @Test func presetsKeepEverythingButThePassword() throws {
        let suite = "InteractionTests-\(UUID().uuidString)"
        let saved = Presets.defaults
        Presets.defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            Presets.defaults = saved
            UserDefaults.standard.removePersistentDomain(forName: suite)
        }
        #expect(Presets.saved().isEmpty)
        let options = CreateOptions(format: .sevenZip, level: .ultra, encryptNames: true, splitBytes: 700 << 20)
        Presets.save([CreatePreset(name: "Discs", options: options, usesPassword: true)])
        let back = try #require(Presets.saved().first)
        #expect(back.name == "Discs" && back.options == options && back.usesPassword)
        #expect(CreateOptions(encoded: options.encoded) == options, "the last options used come back")
    }
}
