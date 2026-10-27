import Foundation
import Testing
@testable import Zipling

/// Making archives in every format, and changing them in place.
@MainActor
struct CreateEditTests {
    private let fm = FileManager.default
    private let readme = String(repeating: "hello create!\n", count: 3000)

    /// Project/docs/readme.txt, Project/Résumé.txt, and Mac clutter: Project/.DS_Store.
    private func makeTree() throws -> URL {
        let root = fm.temporaryDirectory.appending(path: "CreateEditTests-\(UUID().uuidString)")
        let project = root.appending(path: "Project")
        try fm.createDirectory(at: project.appending(path: "docs"), withIntermediateDirectories: true)
        try readme.write(to: project.appending(path: "docs/readme.txt"), atomically: true, encoding: .utf8)
        try "Résumé".write(to: project.appending(path: "Résumé.txt"), atomically: true, encoding: .utf8)
        try Data([0, 0, 0, 1]).write(to: project.appending(path: ".DS_Store"))
        return root
    }

    private func create(_ root: URL, _ name: String, _ options: CreateOptions, password: String? = nil) async throws -> Creation.Made {
        try await Creation.create([root.appending(path: "Project")], into: root.appending(path: name), options: options,
                                  password: password, task: EngineTask(), cancel: CancelToken()) { _, _ in }
    }

    private func paths(_ archive: URL, password: String? = nil, in root: URL) async throws -> Set<String> {
        let opened = try await Archives.open(archive, password: password, scratch: root.appending(path: "scratch"))
        return Set(opened.listing.entries.map(\.path))
    }

    @Test(arguments: [ArchiveFormat.zlg, .zip, .sevenZip, .tarGz, .tarXz])
    func makesEveryFormat(_ format: ArchiveFormat) async throws {
        let root = try makeTree()
        let made = try await create(root, "Project.\(format.fileExtension)", CreateOptions(format: format, level: .fast))
        #expect(made.files.map(\.lastPathComponent) == ["Project.\(format.fileExtension)"])
        #expect(made.result.inputBytes >= UInt64(readme.utf8.count))
        let listed = try await paths(made.files[0], in: root)
        #expect(listed.contains("Project/docs/readme.txt"))
        #expect(listed.contains("Project/Résumé.txt"), "accented names survive")
        if format != .zlg { #expect(!listed.contains("Project/.DS_Store"), "Mac clutter left out") }
        let leftovers = try fm.contentsOfDirectory(atPath: root.path).filter { $0.hasPrefix(".zipling") }
        #expect(leftovers.isEmpty, "nothing left behind")
    }

    @Test func encryptedZipAndHiddenNames() async throws {
        let root = try makeTree()
        let zip = try await create(root, "Secret.zip", CreateOptions(format: .zip), password: "hunter2").files[0]
        let opened = try await Archives.open(zip, password: nil, scratch: root.appending(path: "scratch"))
        #expect(opened.listing.encrypted == true)
        #expect(opened.listing.entries.first { $0.path.hasSuffix("readme.txt") }?.codec.contains("AES-256") == true)
        await #expect(throws: PasswordRequired.self) {
            _ = try await Archives.extract(opened, to: root.appending(path: "out"), only: [], task: EngineTask(),
                                           cancel: CancelToken()) { _, _ in }
        }

        let sevenZip = try await create(root, "Secret.7z", CreateOptions(format: .sevenZip, encryptNames: true),
                                        password: "hunter2").files[0]
        await #expect(throws: PasswordRequired.self, "names are hidden without the password") {
            _ = try await Archives.open(sevenZip, password: nil, scratch: root.appending(path: "scratch"))
        }
        #expect(try await paths(sevenZip, password: "hunter2", in: root).contains("Project/docs/readme.txt"))
    }

    @Test func splitIntoParts() async throws {
        let root = try makeTree()
        var random = SystemRandomNumberGenerator()
        try Data((0..<200_000).map { _ in UInt8.random(in: 0...255, using: &random) })
            .write(to: root.appending(path: "Project/noise.bin"))
        let made = try await create(root, "Parts.zip", CreateOptions(format: .zip, splitBytes: 64_000))
        #expect(made.files.count >= 3)
        #expect(made.files.first?.lastPathComponent == "Parts.zip.001")
        #expect(try await paths(made.files[0], in: root).contains("Project/noise.bin"), "the parts open as one")
        #expect(Archives.volumes(of: made.files[0]).count == made.files.count)
    }

    @Test func adviceFollowsTheContents() {
        var media = ContentSummary(files: 2, bytes: 100)
        media.media = 90
        #expect(Advice(media).options.level == .fast)
        var photos = ContentSummary(files: 2, bytes: 100)
        photos.photos = 60
        #expect(Advice(photos).options == CreateOptions(format: .zlg, level: .balanced))
        #expect(Advice(photos).reason.contains("photos"))
    }

    // MARK: Editing

    private func edit(_ change: Editing.Change, _ archive: URL, password: String? = nil, in root: URL) async throws {
        let opened = try await Archives.open(archive, password: password, scratch: root.appending(path: "scratch"))
        guard case .success(let method) = Editing.method(for: opened, url: archive) else {
            Issue.record("\(archive.lastPathComponent) should be editable")
            return
        }
        try await Editing.apply(change, to: opened, at: archive, method: method, task: EngineTask(), cancel: CancelToken()) { _, _ in }
    }

    @Test(arguments: [ArchiveFormat.zip, .sevenZip, .zlg, .tarGz])
    func editsInPlace(_ format: ArchiveFormat) async throws {
        let root = try makeTree()
        let archive = try await create(root, "Edit.\(format.fileExtension)", CreateOptions(format: format, level: .fast)).files[0]
        let extra = root.appending(path: "added.txt")
        try "added".write(to: extra, atomically: true, encoding: .utf8)

        try await edit(.add([extra], into: "Project/docs"), archive, in: root)
        try await edit(.rename("Project/docs", to: "Project/papers"), archive, in: root)
        try await edit(.delete(["Project/Résumé.txt"]), archive, in: root)
        try await edit(.newFolder("Project/Empty"), archive, in: root)
        let listed = try await paths(archive, in: root)
        #expect(listed.contains("Project/papers/added.txt"), "\(format): added, then moved with its folder")
        #expect(listed.contains("Project/papers/readme.txt"))
        #expect(!listed.contains { $0.hasPrefix("Project/docs") })
        #expect(!listed.contains("Project/Résumé.txt"))
        #expect(listed.contains("Project/Empty"))
        if format == .zlg { #expect(Archives.isZipling(archive), "still a Zipling archive") }

        // Adding a file with the same name replaces it.
        try "changed".write(to: extra, atomically: true, encoding: .utf8)
        try await edit(.add([extra], into: "Project/papers"), archive, in: root)
        let opened = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        let out = root.appending(path: "out")
        _ = try await Archives.extract(opened, to: out, only: ["Project/papers/added.txt"], task: EngineTask(),
                                       cancel: CancelToken()) { _, _ in }
        #expect(try String(contentsOf: out.appending(path: "Project/papers/added.txt"), encoding: .utf8) == "changed")
    }

    @Test func encryptedArchivesStayEncrypted() async throws {
        let root = try makeTree()
        let archive = try await create(root, "Locked.7z", CreateOptions(format: .sevenZip), password: "pw").files[0]
        let extra = root.appending(path: "more.txt")
        try "more".write(to: extra, atomically: true, encoding: .utf8)
        try await edit(.add([extra], into: ""), archive, password: "pw", in: root)
        await #expect(throws: PasswordRequired.self) {
            _ = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        }
        #expect(try await paths(archive, password: "pw", in: root).contains("more.txt"))
    }

    @Test func someArchivesCantBeEdited() async throws {
        let root = try makeTree()
        let made = try await create(root, "Parts.7z", CreateOptions(format: .sevenZip, splitBytes: 2_000))
        let opened = try await Archives.open(made.files[0], password: nil, scratch: root.appending(path: "scratch"))
        guard case .failure(let error) = Editing.method(for: opened, url: made.files[0]) else {
            Issue.record("split archives can't be edited")
            return
        }
        #expect(error.message.contains("Split"))
    }

    @Test func editingThroughTheWindow() async throws {
        let root = try makeTree()
        let archive = try await create(root, "Window.zip", CreateOptions(format: .zip)).files[0]
        let model = ArchiveModel(url: archive)
        await model.load()
        model.show(folder: "Project/docs")
        let extra = root.appending(path: "note.txt")
        try "note".write(to: extra, atomically: true, encoding: .utf8)
        model.add([extra])
        for _ in 0..<1000 where model.job?.isRunning != false { try await Task.sleep(for: .milliseconds(10)) }
        #expect(model.folder == "Project/docs", "stays in the same folder")
        #expect(model.visibleItems.map(\.name).contains("note.txt"))
        #expect(model.selection == ["Project/docs/note.txt"], "the new item is selected")

        let note = try #require(model.item("Project/docs/note.txt"))
        #expect(model.rename(note, to: "a/b") != nil, "slashes refused")
        #expect(model.rename(note, to: "readme.txt") != nil, "taken names refused")
        #expect(model.rename(note, to: "Notes.txt") == nil)
        for _ in 0..<1000 where model.job?.isRunning != false { try await Task.sleep(for: .milliseconds(10)) }
        #expect(model.visibleItems.map(\.name).contains("Notes.txt"))
        model.close()
    }
}
