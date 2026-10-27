import Foundation
import Testing
@testable import Zipling

/// The browser window's behaviour: search inside nested archives, Extract Here's
/// rules, name conflicts, split-archive volumes, keyboard selection, and speed with
/// a hundred thousand entries.
@MainActor
struct BrowserTests {
    private let fm = FileManager.default
    private let readme = String(repeating: "hello browser!\n", count: 2000)

    /// A fresh folder holding Project/docs/readme.txt, Project/docs/nested/copy.txt
    /// and Project/notes.txt.
    private func makeTree() throws -> URL {
        let root = fm.temporaryDirectory.appending(path: "BrowserTests-\(UUID().uuidString)")
        let project = root.appending(path: "Project")
        try fm.createDirectory(at: project.appending(path: "docs/nested"), withIntermediateDirectories: true)
        try readme.write(to: project.appending(path: "docs/readme.txt"), atomically: true, encoding: .utf8)
        try readme.write(to: project.appending(path: "docs/nested/copy.txt"), atomically: true, encoding: .utf8)
        try "notes".write(to: project.appending(path: "notes.txt"), atomically: true, encoding: .utf8)
        return root
    }

    @discardableResult
    private func sh(_ tool: String, _ args: [String], in dir: URL) throws -> Int32 {
        let p = Process()
        p.executableURL = URL(filePath: tool)
        p.arguments = args
        p.currentDirectoryURL = dir
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        try p.run()
        p.waitUntilExit()
        return p.terminationStatus
    }

    private var sevenZip: String { SevenZip.executable!.path }

    private func until(_ condition: @MainActor () -> Bool) async throws {
        for _ in 0..<1000 where !condition() { try await Task.sleep(for: .milliseconds(10)) }
    }

    private func waitForJob(_ model: ArchiveModel) async throws {
        try await until { model.job.map { !$0.isRunning } ?? false }
        if case .failed(let message) = model.job?.phase { Issue.record("job failed: \(message)") }
    }

    @Test func searchLooksInsideNestedArchives() async throws {
        let root = try makeTree()
        try sh(sevenZip, ["a", root.appending(path: "inner.7z").path, "Project"], in: root)
        try "top".write(to: root.appending(path: "top.txt"), atomically: true, encoding: .utf8)
        let outer = root.appending(path: "Outer.zip")
        try sh(sevenZip, ["a", "-tzip", outer.path, "inner.7z", "top.txt"], in: root)

        let model = ArchiveModel(url: outer)
        await model.load()
        model.search = "copy"
        #expect(model.visibleItems.isEmpty, "the outer archive has no copy.txt")
        model.searchInside = true
        try await until { !model.visibleItems.isEmpty }
        let hit = try #require(model.visibleItems.first)
        #expect(hit.name == "copy.txt" && hit.inArchive == "inner.7z")
        #expect(model.location(of: hit) == "inner.7z › Project/docs/nested")

        // A hit comes straight out of the archive it's in.
        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        model.extract([hit.id], to: dest, then: .nothing)
        try await waitForJob(model)
        #expect(try String(contentsOf: dest.appending(path: "copy.txt"), encoding: .utf8) == readme)

        // Opening a hit opens its archive, at the hit.
        model.activate([hit.id])
        try await until { model.levels.count == 2 }
        #expect(model.levelNames == ["Outer.zip", "inner.7z"])
        #expect(model.folder == "Project/docs/nested")
        #expect(model.selection == ["Project/docs/nested/copy.txt"])
        #expect(model.expanded.isSuperset(of: ["", "Project", "Project/docs"]), "the sidebar opens down to it")
        model.close()
    }

    @Test func previewsComeFromTheArchive() async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Project.7z")
        try sh(sevenZip, ["a", archive.path, "Project"], in: root)
        let model = ArchiveModel(url: archive)
        await model.load()
        let readmeItem = try #require(model.levels.last?.byID["Project/docs/readme.txt"])
        let url = try await model.materialize(readmeItem)
        #expect(try String(contentsOf: url, encoding: .utf8) == readme)
        #expect(try await model.materialize(readmeItem) == url, "extracted once, then reused")
        #expect(url.path.hasPrefix(model.scratch.path), "kept in the window's private folder")
        model.close()
    }

    @Test func extractHereKeepsThingsTogether() async throws {
        let root = try makeTree()
        // One folder at the top: it comes out as it is.
        let tidy = root.appending(path: "Tidy.zip")
        try sh(sevenZip, ["a", "-tzip", tidy.path, "Project"], in: root)
        // Several items at the top: they go in a folder named after the archive.
        let loose = root.appending(path: "Loose.tar.gz")
        try sh("/usr/bin/tar", ["--no-mac-metadata", "-czf", loose.path, "-C", "Project", "docs", "notes.txt"], in: root)

        let out = root.appending(path: "out")
        try fm.createDirectory(at: out, withIntermediateDirectories: true)
        for (archive, folder) in [(tidy, "Tidy"), (loose, "Loose")] {
            let model = ArchiveModel(url: archive)
            await model.load()
            model.extract([], to: out, then: .nothing, layout: .smart(folder: Archives.baseName(archive.lastPathComponent)),
                          conflicts: Conflicts(policy: .keepBoth))
            try await waitForJob(model)
            #expect(model.job?.placed.map(\.lastPathComponent) == [folder == "Tidy" ? "Project" : "Loose"])
            model.close()
        }
        #expect(fm.fileExists(atPath: out.appending(path: "Project/docs/readme.txt").path))
        #expect(fm.fileExists(atPath: out.appending(path: "Loose/docs/readme.txt").path))
        #expect(fm.fileExists(atPath: out.appending(path: "Loose/notes.txt").path))

        // Again: Keep Both numbers the new one; Skip leaves the old one alone.
        let model = ArchiveModel(url: tidy)
        await model.load()
        model.extract([], to: out, then: .nothing, layout: .smart(folder: "Tidy"), conflicts: Conflicts(policy: .keepBoth))
        try await waitForJob(model)
        #expect(model.job?.placed.map(\.lastPathComponent) == ["Project 2"])
        model.extract([], to: out, then: .nothing, layout: .smart(folder: "Tidy"), conflicts: Conflicts(policy: .skip))
        try await waitForJob(model)
        #expect(model.job?.placed.isEmpty == true)
        #expect(model.job?.outcome == "Nothing new came out: the item was already there and was kept.")
        #expect(!fm.fileExists(atPath: out.appending(path: "Project 3").path))
        let leftovers = try fm.contentsOfDirectory(atPath: out.path).filter { $0.hasPrefix(".zipling") }
        #expect(leftovers.isEmpty, "no staging folders left behind")
        model.close()
    }

    @Test func splitVolumesAndNames() throws {
        let dir = fm.temporaryDirectory.appending(path: "BrowserTests-\(UUID().uuidString)")
        try fm.createDirectory(at: dir, withIntermediateDirectories: true)
        let names = ["a.7z.001", "a.7z.002", "a.7z.003", "b.part1.rar", "b.part2.rar", "c.zip", "c.z01",
                     "d.rar", "d.r00", "e.zip", "other.txt"]
        for name in names { try Data().write(to: dir.appending(path: name)) }
        func volumes(_ name: String) -> [String] { Archives.volumes(of: dir.appending(path: name)).map(\.lastPathComponent) }
        #expect(volumes("a.7z.001") == ["a.7z.001", "a.7z.002", "a.7z.003"])
        #expect(volumes("b.part1.rar") == ["b.part1.rar", "b.part2.rar"])
        #expect(volumes("c.zip") == ["c.z01", "c.zip"])
        #expect(volumes("d.rar") == ["d.r00", "d.rar"])
        #expect(volumes("e.zip") == ["e.zip"])

        #expect(Archives.baseName("Photos.tar.gz") == "Photos")
        #expect(Archives.baseName("Photos.7z.001") == "Photos")
        #expect(Archives.baseName("Photos.part1.rar") == "Photos")
        #expect(Archives.baseName("Q3.Report.zip") == "Q3.Report")
    }

    @Test func typingSelectsByName() async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Project.zip")
        try sh(sevenZip, ["a", "-tzip", archive.path, "Project"], in: root)
        let model = ArchiveModel(url: archive)
        await model.load()
        model.show(folder: "Project")
        #expect(model.typeSelect("n"))
        #expect(model.selection == ["Project/notes.txt"])
        #expect(!model.typeSelect("\u{F700}"), "arrow keys are left to the list")
        model.goUp()
        #expect(model.folder == "" && model.selection == ["Project"], "going up selects where you came from")
        model.close()
    }

    /// Folders an archive doesn't list get their newest file's date (not 1970), and
    /// files kept as they are read "Stored" whichever tool listed them.
    @Test func implicitFoldersAndStoredLabels() {
        let entry = { (path: String, mtime: Int64) in
            ArchiveEntry(path: path, kind: "file", size: 1, mtime: mtime, mode: 0o644, transform: "foreign",
                         codec: "None", packed: 1, target: nil)
        }
        let (children, _) = ArchiveModel.buildTree([entry("Docs/a.txt", 1_700_000_000), entry("Docs/b.txt", 1_800_000_000)])
        let docs = children[""]?.first { $0.name == "Docs" }
        #expect(docs?.modified == Date(timeIntervalSince1970: 1_800_000_000))
        #expect(Method.foreignName("None") == "Stored")
        #expect(Method.foreignName("Copy") == "Stored")
        #expect(Method.foreignName("LZMA2:24 BCJ") == "LZMA2 + BCJ")
    }

    /// A hundred thousand entries: listing, building the tree, and searching stay quick.
    @Test func hundredThousandEntries() async throws {
        let root = fm.temporaryDirectory.appending(path: "BrowserTests-\(UUID().uuidString)")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        let archive = root.appending(path: "Big.zip")
        let script = """
        import sys, zipfile
        with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_STORED) as z:
            for d in range(100):
                for f in range(1000):
                    z.writestr(f"folder{d:03}/file{f:04}.txt", "x")
        """
        try sh("/usr/bin/python3", ["-c", script, archive.path], in: root)

        let clock = ContinuousClock()
        let model = ArchiveModel(url: archive)
        let opened = try await clock.measure { await model.load() }
        #expect(model.listing?.files == 100_000)
        model.show(folder: "folder042")
        try await until { model.visibleItems.count == 1000 }
        #expect(model.visibleItems.first?.name == "file0000.txt")

        let searched = try await clock.measure {
            model.search = "file0999"
            try await until { model.visibleItems.count == 100 }
        }
        #expect(model.visibleItems.count == 100)
        print("100k entries: opened in \(opened), searched in \(searched)")
        #expect(opened < .seconds(10))
        #expect(searched < .seconds(2))
        model.close()
    }
}
