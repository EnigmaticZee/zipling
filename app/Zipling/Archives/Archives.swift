import Foundation

/// One archive opened for browsing, and which engine reads it.
struct OpenedArchive: Sendable {
    enum Backend: Sendable, Equatable {
        /// Zipling's own `.zlg` format, read by the Arca engine.
        case arca
        /// Most other formats, read by the 7-Zip helper.
        case sevenZip
        /// Classic Mac formats, and archives whose names need a legacy encoding,
        /// read by The Unarchiver's tools.
        case unar
    }

    /// What the backend reads: the archive itself, or the `.tar` inside a `.tar.gz`.
    let source: URL
    let backend: Backend
    let listing: Listing
    /// The password that opened it, reused for extraction and nested archives.
    var password: String?
    /// The file-name encoding in use (The Unarchiver only): detected or chosen.
    var encoding: String?
    /// True when the encoding was picked by hand rather than detected.
    var encodingChosen = false
}

/// The single entry point for reading archives of any format.
enum Archives {
    /// File extensions that are archives in their own right: double-clicking one
    /// inside an archive drills into it instead of opening it in another app.
    static let archiveExtensions: Set<String> = [
        "zlg", "zip", "zipx", "7z", "rar", "tar", "gz", "tgz", "bz2", "tbz", "tbz2", "xz", "txz", "zst", "tzst",
        "lz4", "tlz4", "lzma", "tlz", "z", "taz", "cab", "iso", "dmg", "wim", "swm", "esd", "xar", "pkg", "cpio",
        "rpm", "deb", "ar", "arj", "lzh", "lha", "jar", "war", "apk", "ipa", "xip", "cbz", "cbr", "cb7", "chm",
        "msi", "squashfs", "vhd", "vhdx", "vmdk", "vdi", "qcow2", "001",
    ]

    /// An archive's name without its archive extensions: "Photos" for Photos.tar.gz,
    /// Photos.7z.001 or Photos.part1.rar.
    static func baseName(_ name: String) -> String {
        var base = name
        if let m = base.lowercased().wholeMatch(of: /(.+)\.part\d+\.rar/) { base = String(base.prefix(m.1.count)) }
        while case let ext = (base as NSString).pathExtension.lowercased(), !ext.isEmpty,
              archiveExtensions.contains(ext) || ext.wholeMatch(of: /\d{3}/) != nil {
            base = (base as NSString).deletingPathExtension
        }
        return base.isEmpty ? name : base
    }

    static func looksLikeArchive(_ name: String) -> Bool {
        archiveExtensions.contains((name as NSString).pathExtension.lowercased())
    }

    /// Zipling archives start with the Arca magic, whatever their name.
    static func isZipling(_ url: URL) -> Bool {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? handle.close() }
        return (try? handle.read(upToCount: 8)) == Data("ARCA\u{0}\r\n\u{1a}".utf8)
    }

    /// Opens an archive. Throws `PasswordRequired` if it needs a password to list.
    /// `scratch` is a private folder for unwrapped `.tar` files. `encoding` forces a
    /// file-name encoding (read with The Unarchiver).
    static func open(
        _ url: URL, password: String?, scratch: URL, encoding: String? = nil, cancel: CancelToken? = nil
    ) async throws -> OpenedArchive {
        if isZipling(url) {
            return OpenedArchive(source: url, backend: .arca, listing: try await Engine.list(url))
        }
        if let encoding, Unar.isAvailable {
            let found = try await Unar.list(url, password: password, encoding: encoding, cancel: cancel)
            return OpenedArchive(source: url, backend: .unar, listing: found.listing, password: password,
                                 encoding: encoding, encodingChosen: true)
        }
        let listing: Listing
        do {
            listing = try await SevenZip.list(url, password: password, cancel: cancel)
        } catch let error as EngineError where Unar.isAvailable && !error.isCancellation {
            // Formats 7-Zip doesn't know (StuffIt, BinHex, MacBinary, Compact Pro…).
            guard let found = try? await Unar.list(url, password: password, encoding: nil, cancel: cancel) else { throw error }
            return OpenedArchive(source: url, backend: .unar, listing: found.listing, password: password,
                                 encoding: found.encoding)
        }
        if let better = await betterNames(url, listing: listing, password: password, cancel: cancel) {
            return better
        }
        // A .tar.gz, .tar.xz, .tbz2… is one .tar inside a compressed stream. Open the
        // tar so people see their files, not a single "name.tar".
        if let inner = wrappedTar(listing) {
            // A stream holds exactly one file, so no name filter: xz and bzip2 don't store names.
            let dir = scratch.appending(path: UUID().uuidString)
            try await SevenZip.extract(url, to: dir, only: [], password: password,
                                       totalBytes: inner.size, cancel: cancel ?? CancelToken()) { _, _ in }
            guard let name = SafeExtraction.names(in: dir.path).first else {
                throw EngineError(message: "“\(url.lastPathComponent)” seems to be empty.")
            }
            let tar = dir.appending(path: name)
            var tarListing = try await SevenZip.list(tar, password: nil, cancel: cancel)
            tarListing = Listing(
                archiveBytes: listing.archiveBytes, created: listing.created, files: tarListing.files,
                dirs: tarListing.dirs, totalSize: tarListing.totalSize, blocks: tarListing.blocks,
                entries: tarListing.entries, format: "tar." + shortName(listing.format ?? ""),
                method: listing.method, solid: true, encrypted: listing.encrypted)
            return OpenedArchive(source: tar, backend: .sevenZip, listing: tarListing, password: password)
        }
        return OpenedArchive(source: url, backend: .sevenZip, listing: listing, password: password)
    }

    /// Formats whose file names carry no reliable encoding: Windows tools wrote them
    /// in the PC's code page (Shift-JIS, GBK, CP437…), which 7-Zip shows as garbage.
    static let legacyNameFormats: Set<String> = ["zip", "arj", "lzh", "cab"]

    /// Names in a legacy code page are read with The Unarchiver instead, which detects
    /// the encoding. When it can't tell, ZIP's own default applies: the DOS code page.
    private static func betterNames(_ url: URL, listing: Listing, password: String?, cancel: CancelToken?) async -> OpenedArchive? {
        let format = (listing.format ?? "").lowercased()
        guard Unar.isAvailable, legacyNameFormats.contains(format),
              format != "zip" || ZipDirectory.needsDecoding(url),
              var found = try? await Unar.list(url, password: password, encoding: nil, cancel: cancel) else { return nil }
        // Other formats: only when the names come out differently.
        if format != "zip", Set(found.listing.entries.map(\.path)) == Set(listing.entries.map(\.path)) { return nil }
        var encoding = found.encoding
        if format == "zip", found.confidence < 0.2,
           let dos = try? await Unar.list(url, password: password, encoding: "IBM437", cancel: cancel) {
            found = dos
            encoding = "IBM437"
        }
        return OpenedArchive(source: url, backend: .unar, listing: found.listing, password: password, encoding: encoding)
    }

    /// Opens an archive stored inside another one, by extracting just that entry to
    /// the scratch folder (with the outer archive's quarantine) and opening it there.
    static func openNested(
        _ path: String, in parent: OpenedArchive, scratch: URL, quarantine: Data?, password: String?
    ) async throws -> OpenedArchive {
        let dir = scratch.appending(path: UUID().uuidString)
        _ = try await extract(parent, to: dir, only: [path], task: EngineTask(), cancel: CancelToken()) { _, _ in }
        _ = try SafeExtraction.finalize(dir, quarantine: quarantine)
        return try await open(dir.appending(path: path), password: password ?? parent.password, scratch: scratch)
    }

    /// Extracts everything, or the given paths, into `dest`.
    static func extract(
        _ archive: OpenedArchive, to dest: URL, only: [String], task: EngineTask, cancel: CancelToken,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ExtractResult {
        let (files, bytes) = selectionSize(archive.listing, only: only)
        try ensureSpace(bytes, at: dest.deletingLastPathComponent())
        switch archive.backend {
        case .arca:
            return try await Engine.extract(archive.source, to: dest, only: only, task: task, progress: progress)
        case .unar:
            let started = Date()
            let indexes = only.isEmpty ? nil : archive.listing.entries.filter { e in
                only.contains { e.path == $0 || e.path.hasPrefix($0 + "/") }
            }.flatMap { [$0.index, $0.forkIndex].compactMap { $0 } }
            try await Unar.extract(archive.source, to: dest, indexes: indexes, password: archive.password,
                                   encoding: archive.encoding, totalFiles: files, cancel: cancel) { fraction in
                progress(UInt64(Double(bytes) * fraction), bytes)
            }
            return ExtractResult(files: files, bytes: bytes, seconds: Date().timeIntervalSince(started))
        case .sevenZip:
            let started = Date()
            let skipped = try await SevenZip.extract(archive.source, to: dest, only: only, password: archive.password,
                                                     totalBytes: bytes, cancel: cancel, progress: progress)
            return ExtractResult(files: files - UInt64(min(skipped.count, Int(files))), bytes: bytes,
                                 seconds: Date().timeIntervalSince(started), skipped: skipped)
        }
    }

    /// Decodes everything and checks every checksum, writing nothing.
    static func verify(
        _ archive: OpenedArchive, task: EngineTask, cancel: CancelToken,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> ExtractResult {
        switch archive.backend {
        case .arca:
            return try await Engine.verify(archive.source, task: task, progress: progress)
        case .unar:
            let started = Date()
            let l = archive.listing
            try await Unar.test(archive.source, password: archive.password, encoding: archive.encoding, cancel: cancel)
            progress(l.totalSize, l.totalSize)
            return ExtractResult(files: l.files, bytes: l.totalSize, seconds: Date().timeIntervalSince(started))
        case .sevenZip:
            let started = Date()
            let l = archive.listing
            try await SevenZip.test(archive.source, password: archive.password, totalBytes: l.totalSize,
                                    cancel: cancel, progress: progress)
            return ExtractResult(files: l.files, bytes: l.totalSize, seconds: Date().timeIntervalSince(started))
        }
    }

    /// Files and bytes in the selection (a folder counts everything inside it).
    static func selectionSize(_ listing: Listing, only: [String]) -> (UInt64, UInt64) {
        var files: UInt64 = 0, bytes: UInt64 = 0
        for e in listing.entries where !e.isDirectory {
            if only.isEmpty || only.contains(where: { e.path == $0 || e.path.hasPrefix($0 + "/") }) {
                files += 1
                bytes += e.size
            }
        }
        return (files, bytes)
    }

    /// Refuses to start an extraction that can't fit, instead of failing halfway.
    static func ensureSpace(_ bytes: UInt64, at folder: URL) throws {
        let values = try? folder.resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
        guard let free = values?.volumeAvailableCapacityForImportantUsage, free > 0 else { return }
        if bytes > UInt64(free) {
            throw EngineError(message: "There isn't enough free space: this needs \(Format.bytes(bytes)), and \(Format.bytes(UInt64(free))) is available.")
        }
    }

    /// Every file of a split archive, or just the file: name.7z.001, .002…;
    /// name.zip with name.z01…; name.part1.rar, part2…; name.rar with name.r00….
    static func volumes(of url: URL) -> [URL] {
        let folder = url.deletingLastPathComponent()
        guard let key = volumeGroup(url.lastPathComponent),
              let siblings = try? FileManager.default.contentsOfDirectory(atPath: folder.path) else { return [url] }
        let all = siblings.filter { volumeGroup($0) == key }.sorted().map { folder.appending(path: $0) }
        return all.isEmpty ? [url] : all
    }

    /// Whether a name is one part of a split archive (name.7z.001, name.part1.rar).
    static func isVolume(_ name: String) -> Bool {
        guard let group = volumeGroup(name) else { return false }
        return group.hasPrefix("num:") || group.hasPrefix("part:")
    }

    /// Names that belong to the same split archive share a group.
    private static func volumeGroup(_ name: String) -> String? {
        let lower = name.lowercased()
        if let m = lower.wholeMatch(of: /(.+)\.part\d+\.rar/) { return "part:" + m.1 }
        if let m = lower.wholeMatch(of: /(.+)\.(?:rar|r\d{2})/) { return "rar:" + m.1 }
        if let m = lower.wholeMatch(of: /(.+)\.(?:zip|z\d{2})/) { return "zip:" + m.1 }
        if let m = lower.wholeMatch(of: /(.+)\.\d{3}/) { return "num:" + m.1 }
        return nil
    }

    /// Formats that compress one stream (usually a .tar) rather than holding files.
    static let streamFormats: Set<String> = ["gzip", "bzip2", "xz", "zstd", "lz4", "lzma", "z", "lzip"]

    private static func wrappedTar(_ listing: Listing) -> ArchiveEntry? {
        guard streamFormats.contains((listing.format ?? "").lowercased()), listing.entries.count == 1,
              let only = listing.entries.first, only.path.lowercased().hasSuffix(".tar") else { return nil }
        return only
    }

    private static func shortName(_ format: String) -> String {
        switch format.lowercased() {
        case "gzip": "gz"
        case "bzip2": "bz2"
        case "zstd": "zst"
        default: format.lowercased()
        }
    }
}
