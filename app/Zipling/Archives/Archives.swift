import Foundation

/// One archive opened for browsing, and which engine reads it.
struct OpenedArchive: Sendable {
    enum Backend: Sendable {
        /// Zipling's own `.zlg` format, read by the Arca engine.
        case arca
        /// Every other format, read by the 7-Zip helper.
        case sevenZip
    }

    /// What the backend reads: the archive itself, or the `.tar` inside a `.tar.gz`.
    let source: URL
    let backend: Backend
    let listing: Listing
    /// The password that opened it, reused for extraction and nested archives.
    var password: String?
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
    /// `scratch` is a private folder for unwrapped `.tar` files.
    static func open(_ url: URL, password: String?, scratch: URL, cancel: CancelToken? = nil) async throws -> OpenedArchive {
        if isZipling(url) {
            return OpenedArchive(source: url, backend: .arca, listing: try await Engine.list(url))
        }
        let listing = try await SevenZip.list(url, password: password, cancel: cancel)
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
