import Darwin
import Foundation
import Testing
@testable import Zipling

/// Every archive format through the real 7-Zip helper: listing, extraction,
/// passwords, nested archives, hostile archives, quarantine and Mac metadata.
/// Archives are made with the tools people use: 7-Zip, tar, hdiutil, ditto, Python.
@MainActor
struct FormatTests {
    private let fm = FileManager.default
    private let readme = String(repeating: "hello formats!\n", count: 3000)

    /// A fresh folder holding Project/docs/readme.txt, Project/docs/nested/copy.txt
    /// and Project/Résumé.txt.
    private func makeTree() throws -> URL {
        let root = fm.temporaryDirectory.appending(path: "FormatTests-\(UUID().uuidString)")
        let project = root.appending(path: "Project")
        try fm.createDirectory(at: project.appending(path: "docs/nested"), withIntermediateDirectories: true)
        try readme.write(to: project.appending(path: "docs/readme.txt"), atomically: true, encoding: .utf8)
        try readme.write(to: project.appending(path: "docs/nested/copy.txt"), atomically: true, encoding: .utf8)
        try "Résumé".write(to: project.appending(path: "Résumé.txt"), atomically: true, encoding: .utf8)
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

    private func extractAll(_ archive: OpenedArchive, into dir: URL) async throws {
        _ = try await Archives.extract(archive, to: dir, only: [], task: EngineTask(), cancel: CancelToken()) { _, _ in }
    }

    private func readmePath(_ listing: Listing) -> String? {
        listing.entries.first { $0.path.hasSuffix("docs/readme.txt") }?.path
    }

    @Test func helperIsAvailable() {
        #expect(SevenZip.executable != nil, "the bundled 7zz (or a Homebrew one in debug builds)")
    }

    @Test(arguments: ["zip", "7z", "tar", "tar.gz", "tar.bz2", "tar.xz", "wim", "dmg-hfs", "dmg-apfs", "iso"])
    func listsAndExtracts(_ kind: String) async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Project.\(kind.hasPrefix("dmg") ? "dmg" : kind)")
        switch kind {
        case "zip", "7z", "wim":
            try sh(sevenZip, ["a", "-t\(kind)", archive.path, "Project"], in: root)
        case "tar": try sh("/usr/bin/tar", ["--no-mac-metadata", "-cf", archive.path, "Project"], in: root)
        case "tar.gz": try sh("/usr/bin/tar", ["--no-mac-metadata", "-czf", archive.path, "Project"], in: root)
        case "tar.bz2": try sh("/usr/bin/tar", ["--no-mac-metadata", "-cjf", archive.path, "Project"], in: root)
        case "tar.xz": try sh("/usr/bin/tar", ["--no-mac-metadata", "-cJf", archive.path, "Project"], in: root)
        case "dmg-hfs", "dmg-apfs":
            try sh("/usr/bin/hdiutil", ["create", "-quiet", "-srcfolder", "Project", "-volname", "Project",
                                        "-fs", kind == "dmg-hfs" ? "HFS+" : "APFS", "-format", "UDZO", archive.path], in: root)
        case "iso": try sh("/usr/bin/hdiutil", ["makehybrid", "-quiet", "-iso", "-joliet", "-o", archive.path, "Project"], in: root)
        default: Issue.record("unknown kind")
        }
        #expect(fm.fileExists(atPath: archive.path), "\(kind) archive was created")

        let opened = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        let listing = opened.listing
        let path = try #require(readmePath(listing), "\(kind): readme listed")
        #expect(listing.entries.first { $0.path == path }?.size == UInt64(readme.utf8.count))
        #expect(listing.entries.contains { $0.path.hasSuffix("Résumé.txt") }, "\(kind): accented names survive")
        #expect(!listing.entries.contains { $0.path.contains("HFS+ Private") || $0.path.hasPrefix("._") })
        if kind.hasPrefix("tar.") { #expect(listing.format == kind, "a .\(kind) opens as its tar") }
        if kind == "zip" { #expect(opened.backend == .sevenZip, "Unicode names need no second reader") }

        let out = root.appending(path: "out")
        try await extractAll(opened, into: out)
        #expect(try String(contentsOf: out.appending(path: path), encoding: .utf8) == readme, "\(kind): byte-identical")

        let verified = try await Archives.verify(opened, task: EngineTask(), cancel: CancelToken()) { _, _ in }
        #expect(verified.files == listing.files)
    }

    /// RAR can't be created on a Mac, so these are samples from libarchive's test suite.
    @Test(arguments: ["rar4-basic.rar", "rar5-symlinks.rar"])
    func readsRar(_ name: String) async throws {
        let root = try makeTree()
        let fixture = root.appending(path: name)
        try (name == "rar4-basic.rar" ? Fixtures.rar4Basic : Fixtures.rar5Symlinks).write(to: fixture)
        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let model = ArchiveModel(url: fixture)
        await model.load()
        #expect(model.listing?.format?.lowercased().hasPrefix("rar") == true, "\(name) opens as RAR")
        model.extract([], to: dest, then: .nothing)
        try await waitForJob(model)
        if name == "rar4-basic.rar" {
            #expect(try String(contentsOf: dest.appending(path: "testdir/test.txt"), encoding: .utf8).isEmpty == false)
            var isDir: ObjCBool = false
            #expect(fm.fileExists(atPath: dest.appending(path: "testemptydir").path, isDirectory: &isDir) && isDir.boolValue,
                    "empty folders come back")
        } else {
            // Links that point inside the archive survive the safety pass.
            #expect(try fm.destinationOfSymbolicLink(atPath: dest.appending(path: "symlink.txt").path) == "file.txt")
            #expect(try fm.destinationOfSymbolicLink(atPath: dest.appending(path: "dirlink").path) == "dir")
        }
    }

    /// Old Windows zip tools wrote names in the PC's code page, with no marker saying
    /// which. 7-Zip shows them as garbage; Zipling detects the encoding.
    @Test(arguments: ["ja", "zh", "dos"])
    func legacyNamesAreDecoded(_ language: String) async throws {
        func cf(_ e: CFStringEncodings) -> String.Encoding {
            String.Encoding(rawValue: CFStringConvertEncodingToNSStringEncoding(CFStringEncoding(e.rawValue)))
        }
        let (encoding, unarName, names): (String.Encoding, String, [String]) = switch language {
        case "ja": (.shiftJIS, "Shift_JIS", ["資料/会議の議事録.txt", "写真/夏休みの思い出.txt", "請求書と見積書.txt"])
        case "zh": (cf(.GB_18030_2000), "GB18030", ["文档/会议记录和总结.txt", "照片/暑假旅行的回忆.txt", "发票和报价单.txt"])
        // Too little text to detect: ZIP's default, the DOS code page, applies.
        default: (cf(.dosLatinUS), "IBM437", ["Übersicht für März.txt", "Größe/Straße.txt", "Café.txt"])
        }
        let root = try makeTree()
        let zip = root.appending(path: "legacy.zip")
        try legacyZip(names, encoding: encoding).write(to: zip)

        let opened = try await Archives.open(zip, password: nil, scratch: root.appending(path: "scratch"))
        #expect(opened.backend == .unar, "read with The Unarchiver")
        #expect(Set(opened.listing.entries.filter { !$0.isDirectory }.map(\.path)) == Set(names))

        let out = root.appending(path: "out")
        try await extractAll(opened, into: out)
        for name in names {
            #expect(try String(contentsOf: out.appending(path: name), encoding: .utf8) == "contents of \(name)")
        }
        if language == "dos" { #expect(opened.encoding == "IBM437") }
        let one = root.appending(path: "one")
        _ = try await Archives.extract(opened, to: one, only: [names[1]], task: EngineTask(), cancel: CancelToken()) { _, _ in }
        #expect(fm.fileExists(atPath: one.appending(path: names[1]).path), "a single file by index")
        #expect(!fm.fileExists(atPath: one.appending(path: names[2]).path))

        let picked = try await Archives.open(zip, password: nil, scratch: root.appending(path: "scratch"), encoding: unarName)
        #expect(picked.encodingChosen && Set(picked.listing.entries.map(\.path)).isSuperset(of: names))
    }

    /// A stored ZIP whose names are raw bytes in `encoding` with the UTF-8 flag off.
    private func legacyZip(_ names: [String], encoding: String.Encoding) throws -> Data {
        var local = Data(), central = Data()
        func le<T: FixedWidthInteger>(_ value: T, _ out: inout Data) {
            withUnsafeBytes(of: value.littleEndian) { out.append(contentsOf: $0) }
        }
        for name in names {
            let raw = try #require(name.data(using: encoding)), data = Data("contents of \(name)".utf8)
            let crc = crc32(data), offset = UInt32(local.count), size = UInt32(data.count)
            le(UInt32(0x04034b50), &local); le(UInt16(20), &local); le(UInt16(0), &local); le(UInt16(0), &local)
            le(UInt16(0), &local); le(UInt16(0x21), &local); le(crc, &local); le(size, &local); le(size, &local)
            le(UInt16(raw.count), &local); le(UInt16(0), &local)
            local.append(raw); local.append(data)
            le(UInt32(0x02014b50), &central); le(UInt16(20), &central); le(UInt16(20), &central)
            le(UInt16(0), &central); le(UInt16(0), &central); le(UInt16(0), &central); le(UInt16(0x21), &central)
            le(crc, &central); le(size, &central); le(size, &central); le(UInt16(raw.count), &central)
            for _ in 0..<4 { le(UInt16(0), &central) }
            le(UInt32(0x20), &central); le(offset, &central)
            central.append(raw)
        }
        var end = Data()
        le(UInt32(0x06054b50), &end); le(UInt32(0), &end)
        le(UInt16(names.count), &end); le(UInt16(names.count), &end)
        le(UInt32(central.count), &end); le(UInt32(local.count), &end); le(UInt16(0), &end)
        return local + central + end
    }

    private func crc32(_ data: Data) -> UInt32 {
        var crc: UInt32 = 0xffff_ffff
        for byte in data {
            crc ^= UInt32(byte)
            for _ in 0..<8 { crc = crc & 1 == 1 ? (crc >> 1) ^ 0xedb8_8320 : crc >> 1 }
        }
        return ~crc
    }

    /// MacBinary, a classic Mac format 7-Zip doesn't read: the file comes back with its
    /// resource fork and Finder type, including when it's extracted on its own.
    @Test func classicMacBinary() async throws {
        let root = try makeTree()
        let data = Data(String(repeating: "Hello from 1994\r", count: 10).utf8), fork = Data("resource bytes".utf8)
        var header = Data(count: 128)
        let name = Data("Read Me".utf8)
        header[1] = UInt8(name.count)
        header.replaceSubrange(2..<(2 + name.count), with: name)
        header.replaceSubrange(65..<73, with: Data("TEXTttxt".utf8))
        func be32(_ v: UInt32, at i: Int) { withUnsafeBytes(of: v.bigEndian) { header.replaceSubrange(i..<(i + 4), with: $0) } }
        be32(UInt32(data.count), at: 83); be32(UInt32(fork.count), at: 87)
        be32(3_000_000_000, at: 91); be32(3_000_000_000, at: 95)
        header[122] = 129; header[123] = 129
        var crc: UInt16 = 0  // CRC-16/XMODEM of the first 124 bytes
        for byte in header.prefix(124) {
            crc ^= UInt16(byte) << 8
            for _ in 0..<8 { crc = crc & 0x8000 != 0 ? (crc << 1) ^ 0x1021 : crc << 1 }
        }
        header[124] = UInt8(crc >> 8); header[125] = UInt8(crc & 0xff)
        let pad = { (d: Data) in d + Data(count: (128 - d.count % 128) % 128) }
        let file = root.appending(path: "Read Me.bin")
        try (header + pad(data) + pad(fork)).write(to: file)

        let model = ArchiveModel(url: file)
        await model.load()
        #expect(model.listing?.format == "MacBinary")
        #expect(model.listing?.entries.map(\.path) == ["Read Me"], "the fork isn't listed as its own file")
        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        model.extract(["Read Me"], to: dest, then: .nothing)
        try await waitForJob(model)
        let out = dest.appending(path: "Read Me")
        #expect(try Data(contentsOf: out) == data)
        #expect(SafeExtraction.xattr("com.apple.ResourceFork", of: out.path) == fork)
        #expect(SafeExtraction.xattr("com.apple.FinderInfo", of: out.path)?.prefix(8) == Data("TEXTttxt".utf8))
    }

    @Test func zstandardTar() async throws {
        let zstd = ["/opt/homebrew/bin/zstd", "/usr/local/bin/zstd"].first { fm.isExecutableFile(atPath: $0) }
        guard let zstd else { return }  // no zstd tool to make the sample with
        let root = try makeTree()
        try sh("/usr/bin/tar", ["--no-mac-metadata", "-cf", "Project.tar", "Project"], in: root)
        try sh(zstd, ["-q", "--rm", "Project.tar"], in: root)
        let opened = try await Archives.open(root.appending(path: "Project.tar.zst"), password: nil,
                                             scratch: root.appending(path: "scratch"))
        #expect(opened.listing.format == "tar.zst")
        let path = try #require(readmePath(opened.listing))
        try await extractAll(opened, into: root.appending(path: "out"))
        #expect(try String(contentsOf: root.appending(path: "out").appending(path: path), encoding: .utf8) == readme)
    }

    /// A 7z split into parts opens from its first part, and extracts across all of them.
    @Test func splitArchives() async throws {
        let root = try makeTree()
        var rng = SystemRandomNumberGenerator()
        let noise = Data((0..<400_000).map { _ in UInt8.random(in: 0...255, using: &rng) })
        try noise.write(to: root.appending(path: "Project/noise.bin"))
        try sh(sevenZip, ["a", "-v150k", "-mx0", root.appending(path: "Split.7z").path, "Project"], in: root)
        #expect(fm.fileExists(atPath: root.appending(path: "Split.7z.003").path), "split into several parts")
        let opened = try await Archives.open(root.appending(path: "Split.7z.001"), password: nil,
                                             scratch: root.appending(path: "scratch"))
        #expect(opened.listing.entries.contains { $0.path == "Project/noise.bin" })
        try await extractAll(opened, into: root.appending(path: "out"))
        #expect(try Data(contentsOf: root.appending(path: "out/Project/noise.bin")) == noise)
    }

    @Test func selectiveExtraction() async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Project.7z")
        try sh(sevenZip, ["a", archive.path, "Project"], in: root)
        let opened = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        let out = root.appending(path: "out")
        _ = try await Archives.extract(opened, to: out, only: ["Project/docs/nested"], task: EngineTask(), cancel: CancelToken()) { _, _ in }
        #expect(fm.fileExists(atPath: out.appending(path: "Project/docs/nested/copy.txt").path))
        #expect(!fm.fileExists(atPath: out.appending(path: "Project/docs/readme.txt").path), "only the selected folder")
        #expect(!fm.fileExists(atPath: out.appending(path: "Project/Résumé.txt").path))
    }

    @Test func encrypted7zNeedsPasswordToList() async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Secret.7z")
        try sh(sevenZip, ["a", "-psecret", "-mhe=on", archive.path, "Project"], in: root)
        let scratch = root.appending(path: "scratch")

        await #expect { _ = try await Archives.open(archive, password: nil, scratch: scratch) } throws: {
            ($0 as? PasswordRequired)?.wrong == false
        }
        await #expect { _ = try await Archives.open(archive, password: "wrong", scratch: scratch) } throws: {
            ($0 as? PasswordRequired)?.wrong == true
        }
        let opened = try await Archives.open(archive, password: "secret", scratch: scratch)
        #expect(opened.listing.encrypted == true)
        try await extractAll(opened, into: root.appending(path: "out"))
        #expect(fm.fileExists(atPath: root.appending(path: "out/Project/docs/readme.txt").path))
    }

    @Test func aesZipNeedsPasswordToExtract() async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Secret.zip")
        try sh(sevenZip, ["a", "-tzip", "-psecret", "-mem=AES256", archive.path, "Project"], in: root)
        var opened = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        #expect(opened.listing.entries.contains { $0.encrypted == true }, "ZIP names are visible, data is not")
        await #expect { try await extractAll(opened, into: root.appending(path: "out1")) } throws: {
            ($0 as? PasswordRequired)?.wrong == false
        }
        opened.password = "secret"
        try await extractAll(opened, into: root.appending(path: "out2"))
        #expect(try String(contentsOf: root.appending(path: "out2/Project/docs/readme.txt"), encoding: .utf8) == readme)
    }

    @Test func nestedArchivesOpenInPlace() async throws {
        let root = try makeTree()
        try sh(sevenZip, ["a", root.appending(path: "inner.7z").path, "Project"], in: root)
        let outer = root.appending(path: "Outer.zip")
        try sh(sevenZip, ["a", "-tzip", outer.path, "inner.7z"], in: root)

        let model = ArchiveModel(url: outer)
        await model.load()
        let inner = try #require(model.visibleItems.first { $0.name == "inner.7z" })
        #expect(inner.isArchive)
        model.activate([inner.id])
        for _ in 0..<500 where model.levels.count < 2 { try await Task.sleep(for: .milliseconds(10)) }
        #expect(model.levelNames == ["Outer.zip", "inner.7z"])
        #expect(model.visibleItems.map(\.name) == ["Project"])
        model.goUp()
        #expect(model.levelNames == ["Outer.zip"], "Back leaves the nested archive")
        model.close()
        #expect(!fm.fileExists(atPath: model.scratch.path), "the private working folder is removed")
    }

    // MARK: Safety

    /// A ZIP whose entries try to escape: "../evil.txt" and an absolute path.
    @Test func hostileZipStaysInside() async throws {
        let root = try makeTree()
        let abs = fm.temporaryDirectory.appending(path: "zipling-abs-\(UUID().uuidString).txt").path
        let archive = root.appending(path: "Hostile.zip")
        let script = """
        import zipfile, sys
        with zipfile.ZipFile(sys.argv[1], "w") as z:
            z.writestr("../evil.txt", "escaped")
            z.writestr(sys.argv[2], "absolute")
            z.writestr("fine.txt", "fine")
        """
        try sh("/usr/bin/python3", ["-c", script, archive.path, abs], in: root)
        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let model = ArchiveModel(url: archive)
        await model.load()
        model.extract([], to: dest, then: .nothing)
        try await waitForJob(model)
        #expect(!fm.fileExists(atPath: root.appending(path: "evil.txt").path), "nothing above the destination")
        #expect(!fm.fileExists(atPath: abs), "nothing at the absolute path")
        #expect(fm.fileExists(atPath: dest.appending(path: "fine.txt").path))
    }

    /// A tar with a symlink pointing out of the destination, and a file written "through" it.
    @Test func hostileSymlinkStaysInside() async throws {
        let root = try makeTree()
        let outside = root.appending(path: "outside")
        try fm.createDirectory(at: outside, withIntermediateDirectories: true)
        let archive = root.appending(path: "Hostile.tar")
        let script = """
        import tarfile, io, sys
        with tarfile.open(sys.argv[1], "w") as t:
            link = tarfile.TarInfo("escape"); link.type = tarfile.SYMTYPE; link.linkname = sys.argv[2]
            t.addfile(link)
            data = b"pwned"; f = tarfile.TarInfo("escape/pwned.txt"); f.size = len(data)
            t.addfile(f, io.BytesIO(data))
            ok = b"ok"; g = tarfile.TarInfo("fine.txt"); g.size = len(ok)
            t.addfile(g, io.BytesIO(ok))
        """
        try sh("/usr/bin/python3", ["-c", script, archive.path, outside.path], in: root)
        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let model = ArchiveModel(url: archive)
        await model.load()
        model.extract([], to: dest, then: .nothing)
        try await waitForJob(model)
        #expect(!fm.fileExists(atPath: outside.appending(path: "pwned.txt").path), "no write through the link")
        var info = stat()
        let linkLeft = lstat(dest.appending(path: "escape").path, &info) == 0 && info.st_mode & S_IFMT == S_IFLNK
        #expect(!linkLeft, "the escaping link is not left behind")
        #expect(fm.fileExists(atPath: dest.appending(path: "fine.txt").path))
    }

    /// Files from a downloaded archive stay "downloaded", so Gatekeeper still checks them.
    @Test func quarantineFollowsExtractedFiles() async throws {
        let root = try makeTree()
        let archive = root.appending(path: "Downloaded.zip")
        try sh(sevenZip, ["a", "-tzip", archive.path, "Project"], in: root)
        let value = "0083;\(String(format: "%08x", Int(Date().timeIntervalSince1970)));Safari;"
        try sh("/usr/bin/xattr", ["-w", SafeExtraction.quarantineName, value, archive.path], in: root)
        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let model = ArchiveModel(url: archive)
        await model.load()
        model.extract([], to: dest, then: .nothing)
        try await waitForJob(model)
        for path in ["Project", "Project/docs/readme.txt", "Project/Résumé.txt"] {
            let got = SafeExtraction.xattr(SafeExtraction.quarantineName, of: dest.appending(path: path).path)
            #expect(got.map { String(decoding: $0, as: UTF8.self) } == value, "\(path) is quarantined")
        }
    }

    /// A ZIP made by Finder/ditto keeps Mac metadata in __MACOSX; it comes back as
    /// real metadata (here a Finder tag), and no __MACOSX folder or ._ file is left.
    @Test func macMetadataComesBack() async throws {
        let root = try makeTree()
        let tagged = root.appending(path: "Project/docs/readme.txt")
        let tag = "com.apple.metadata:_kMDItemUserTags"
        let plist = try PropertyListSerialization.data(fromPropertyList: ["Red\n6"], format: .binary, options: 0)
        _ = plist.withUnsafeBytes { setxattr(tagged.path, tag, $0.baseAddress, plist.count, 0, 0) }
        let archive = root.appending(path: "Finder.zip")
        try sh("/usr/bin/ditto", ["-c", "-k", "--sequesterRsrc", "--keepParent", "Project", archive.path], in: root)

        let dest = root.appending(path: "dest")
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let model = ArchiveModel(url: archive)
        await model.load()
        #expect(!(model.listing?.entries.contains { $0.path.contains("__MACOSX") } ?? true), "metadata isn't listed")
        model.extract([], to: dest, then: .nothing)
        try await waitForJob(model)
        #expect(SafeExtraction.xattr(tag, of: dest.appending(path: "Project/docs/readme.txt").path) == plist, "tag restored")
        #expect(!fm.fileExists(atPath: dest.appending(path: "__MACOSX").path))
        let leftovers = fm.enumerator(atPath: dest.path)?.allObjects.compactMap { $0 as? String }
            .filter { ($0 as NSString).lastPathComponent.hasPrefix("._") } ?? []
        #expect(leftovers.isEmpty, "no ._ files")
    }

    /// Metadata applied from memory matches what macOS itself restores (`ditto -x -k`)
    /// on every item: tags, custom attributes, Finder info, resource forks, hidden flags.
    @Test func macMetadataMatchesDitto() async throws {
        let root = try makeTree()
        let project = root.appending(path: "Project")
        func set(_ name: String, _ value: Data, on path: String) {
            _ = value.withUnsafeBytes { setxattr(project.appending(path: path).path, name, $0.baseAddress, value.count, 0, 0) }
        }
        let tags = try PropertyListSerialization.data(fromPropertyList: ["Red\n6", "Work"], format: .binary, options: 0)
        set("com.apple.metadata:_kMDItemUserTags", tags, on: "docs/readme.txt")
        set("com.apple.metadata:_kMDItemUserTags", tags, on: "docs/nested")
        set("org.example.note", Data("kept as is".utf8), on: "Résumé.txt")
        set("org.example.big", Data((0..<70_000).map { UInt8($0 % 251) }), on: "Résumé.txt")
        var finderInfo = Data("TEXTttxt".utf8) + Data(count: 24)
        finderInfo[8] = 0x40  // invisible
        set(AppleDouble.finderInfoName, finderInfo, on: "docs/nested/copy.txt")
        set(AppleDouble.resourceForkName, Data((0..<5_000).map { UInt8($0 % 256) }), on: "docs/readme.txt")
        let archive = root.appending(path: "Finder.zip")
        try sh("/usr/bin/ditto", ["-c", "-k", "--sequesterRsrc", "--keepParent", "Project", archive.path], in: root)

        let ours = root.appending(path: "ours"), apple = root.appending(path: "apple")
        try fm.createDirectory(at: ours, withIntermediateDirectories: true)
        try sh("/usr/bin/ditto", ["-x", "-k", archive.path, apple.path], in: root)
        let model = ArchiveModel(url: archive)
        await model.load()
        #expect(model.listing?.macMetadata?.isEmpty == false, "the metadata files are found")
        model.extract([], to: ours, then: .nothing)
        try await waitForJob(model)

        func attributes(_ path: String) -> [String: Data] {
            let size = listxattr(path, nil, 0, XATTR_NOFOLLOW)
            guard size > 0 else { return [:] }
            var names = [CChar](repeating: 0, count: size)
            listxattr(path, &names, size, XATTR_NOFOLLOW)
            let all = names.split(separator: 0).map { String(decoding: $0.map(UInt8.init), as: UTF8.self) }
            // Set by the system on whatever writes a file, so they differ between tools.
            let systemOwned: Set<String> = ["com.apple.provenance", SafeExtraction.quarantineName]
            return Dictionary(uniqueKeysWithValues: all.filter { !systemOwned.contains($0) }.map {
                ($0, SafeExtraction.xattr($0, of: path) ?? Data())
            })
        }
        func hidden(_ path: String) -> Bool {
            var info = stat()
            return lstat(path, &info) == 0 && info.st_flags & UInt32(UF_HIDDEN) != 0
        }
        let items = ["Project", "Project/docs", "Project/docs/nested", "Project/docs/readme.txt",
                     "Project/docs/nested/copy.txt", "Project/Résumé.txt"]
        for item in items {
            let (a, b) = (ours.appending(path: item).path, apple.appending(path: item).path)
            #expect(attributes(a) == attributes(b), "\(item) has the same metadata as ditto gives it")
            #expect(hidden(a) == hidden(b), "\(item) is hidden only if ditto hides it")
        }
        #expect(attributes(ours.appending(path: "Project/docs/readme.txt").path)[AppleDouble.resourceForkName]?.count == 5_000)
        #expect(hidden(ours.appending(path: "Project/docs/nested/copy.txt").path))
        let leftovers = fm.enumerator(atPath: ours.path)?.allObjects.compactMap { $0 as? String }
            .filter { ($0 as NSString).lastPathComponent.hasPrefix("._") || $0.contains("__MACOSX") } ?? []
        #expect(leftovers.isEmpty, "no ._ files or __MACOSX folder")
    }

    private func waitForJob(_ model: ArchiveModel) async throws {
        for _ in 0..<2000 {
            if let job = model.job, !job.isRunning {
                if case .failed(let message) = job.phase { Issue.record("job failed: \(message)") }
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
        Issue.record("job did not finish")
    }
}
