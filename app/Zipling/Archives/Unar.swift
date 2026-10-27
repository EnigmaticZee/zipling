import Foundation

/// The Unarchiver's command-line tools (`lsar` and `unar`, from XADMaster), run
/// unmodified as separate processes. They read the classic Mac and long-tail formats
/// 7-Zip doesn't (StuffIt, BinHex, MacBinary, Compact Pro, DiskDoubler, ARC, Zoo…),
/// and they detect legacy file-name encodings (Shift-JIS, GB18030, Big5, CP437…) that
/// 7-Zip shows as garbage.
enum Unar {
    /// Encodings offered when people pick one by hand: (name for unar, label).
    static let encodings: [(String, String)] = [
        ("UTF-8", "Unicode (UTF-8)"), ("Shift_JIS", "Japanese (Shift JIS)"), ("EUC-JP", "Japanese (EUC)"),
        ("GB18030", "Chinese, Simplified (GB18030)"), ("Big5", "Chinese, Traditional (Big5)"),
        ("EUC-KR", "Korean (EUC-KR)"), ("windows-1251", "Cyrillic (Windows)"), ("IBM866", "Cyrillic (DOS)"),
        ("windows-1253", "Greek (Windows)"), ("windows-1254", "Turkish (Windows)"),
        ("windows-1255", "Hebrew (Windows)"), ("windows-1256", "Arabic (Windows)"),
        ("windows-1252", "Western (Windows)"), ("IBM437", "Western (DOS)"),
    ]

    static func tool(_ name: String) -> URL? {
        let fm = FileManager.default
        let bundled = Bundle.main.bundleURL.appending(path: "Contents/Helpers/\(name)")
        if fm.isExecutableFile(atPath: bundled.path) { return bundled }
        #if DEBUG
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] where fm.isExecutableFile(atPath: "\(dir)/\(name)") {
            return URL(filePath: "\(dir)/\(name)")
        }
        #endif
        return nil
    }

    static var isAvailable: Bool { tool("lsar") != nil && tool("unar") != nil }

    /// A readable name for an encoding lsar reports or people pick.
    static func label(_ encoding: String) -> String {
        encodings.first { $0.0.caseInsensitiveCompare(encoding) == .orderedSame }?.1 ?? encoding
    }

    struct Detected {
        let listing: Listing
        /// The encoding lsar settled on, and how sure it was (0–1).
        let encoding: String?
        let confidence: Double
    }

    /// Lists an archive. With no `encoding`, lsar detects one.
    static func list(_ archive: URL, password: String?, encoding: String?, cancel: CancelToken? = nil) async throws -> Detected {
        guard let lsar = tool("lsar") else { throw EngineError(message: "The Unarchiver's tools are missing. Please reinstall Zipling.") }
        var args = ["-j", archive.path]
        if let encoding { args += ["-e", encoding] }
        if let password { args += ["-p", password] }
        let out = try await SevenZip.run(arguments: args, executable: lsar, password: nil, cancel: cancel)
        if cancel?.isCancelled == true { throw EngineError(message: "cancelled") }
        guard out.status == 0, let json = try? JSONSerialization.jsonObject(with: Data(out.stdout.utf8)) as? [String: Any] else {
            try failure(out, archive: archive, password: password)
        }
        return parse(json, archive: archive)
    }

    static func parse(_ json: [String: Any], archive: URL) -> Detected {
        let contents = json["lsarContents"] as? [[String: Any]] ?? []
        var entries: [ArchiveEntry] = []
        var files: UInt64 = 0, dirs: UInt64 = 0, total: UInt64 = 0, packedTotal: UInt64 = 0
        var anyEncrypted = false
        // Resource forks are listed separately but written back into their file, so
        // they're hidden here and extracted alongside it.
        var forks: [String: Int] = [:]
        for e in contents where (e["XADIsResourceFork"] as? Int) == 1 {
            forks[SevenZip.cleanPath(e["XADFileName"] as? String ?? "")] = e["XADIndex"] as? Int
        }
        for e in contents {
            if (e["XADIsResourceFork"] as? Int) == 1 { continue }
            let path = SevenZip.cleanPath(e["XADFileName"] as? String ?? "")
            guard !path.isEmpty, !SevenZip.isAppleDouble(path) else { continue }
            let isDir = (e["XADIsDirectory"] as? Int) == 1
            let link = e["XADLinkDestination"] as? String
            let kind = isDir ? "dir" : link != nil ? "link" : "file"
            let size = (e["XADFileSize"] as? NSNumber)?.uint64Value ?? 0
            let packed = (e["XADCompressedSize"] as? NSNumber)?.uint64Value
            let encrypted = (e["XADIsEncrypted"] as? Int) == 1
            anyEncrypted = anyEncrypted || encrypted
            if isDir { dirs += 1 } else { files += 1; total += size; packedTotal += packed ?? 0 }
            entries.append(ArchiveEntry(
                path: path, kind: kind, size: size, mtime: timestamp(e["XADLastModificationDate"] as? String),
                mode: UInt32((e["XADPosixPermissions"] as? Int) ?? 0o644) & 0o777, transform: "foreign",
                codec: e["XADCompressionName"] as? String ?? "", packed: packed ?? 0, target: link,
                packedKnown: packed != nil && kind == "file", encrypted: encrypted, index: e["XADIndex"] as? Int,
                forkIndex: forks[path]))
        }
        let attributes = try? FileManager.default.attributesOfItem(atPath: archive.path)
        let fileBytes = (attributes?[.size] as? NSNumber)?.uint64Value ?? packedTotal
        let modified = (attributes?[.modificationDate] as? Date) ?? .now
        let listing = Listing(
            archiveBytes: fileBytes, created: Int64(modified.timeIntervalSince1970), files: files, dirs: dirs,
            totalSize: total, blocks: 0, entries: entries, format: json["lsarFormatName"] as? String ?? "archive",
            method: nil, solid: nil, encrypted: anyEncrypted)
        return Detected(listing: listing, encoding: json["lsarEncoding"] as? String,
                        confidence: (json["lsarConfidence"] as? NSNumber)?.doubleValue ?? 0)
    }

    private static let dateParser: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd HH:mm:ss Z"
        return f
    }()

    static func timestamp(_ text: String?) -> Int64 {
        guard let text, let date = dateParser.date(from: text) else { return 0 }
        return Int64(date.timeIntervalSince1970)
    }

    /// Extracts the entries with the given lsar indexes (all of them when nil).
    static func extract(
        _ archive: URL, to dest: URL, indexes: [Int]?, password: String?, encoding: String?, totalFiles: UInt64,
        cancel: CancelToken, progress: @escaping @Sendable (Double) -> Void
    ) async throws {
        guard let unar = tool("unar") else { throw EngineError(message: "The Unarchiver's tools are missing. Please reinstall Zipling.") }
        try FileManager.default.createDirectory(at: dest, withIntermediateDirectories: true)
        // -D: no extra enclosing folder; -nr: never unpack archives found inside;
        // -f: the staging folder is new, so nothing real is overwritten.
        var args = ["-o", dest.path, "-D", "-nr", "-f"]
        if let encoding { args += ["-e", encoding] }
        if let password { args += ["-p", password] }
        if let indexes {
            args += ["-i", archive.path] + indexes.map(String.init)
        } else {
            args.append(archive.path)
        }
        let counter = LineCounter(total: max(totalFiles, 1), progress: progress)
        let out = try await SevenZip.run(arguments: args, executable: unar, password: nil, cancel: cancel,
                                         lines: { counter.saw($0) })
        if cancel.isCancelled { throw EngineError(message: "cancelled") }
        if out.status != 0 || out.stdout.contains("Failed!") || out.stdout.contains("failed") {
            try failure(out, archive: archive, password: password)
        }
    }

    /// Checks every checksum without writing anything.
    static func test(_ archive: URL, password: String?, encoding: String?, cancel: CancelToken) async throws {
        guard let lsar = tool("lsar") else { throw EngineError(message: "The Unarchiver's tools are missing. Please reinstall Zipling.") }
        var args = ["-t", archive.path]
        if let encoding { args += ["-e", encoding] }
        if let password { args += ["-p", password] }
        let out = try await SevenZip.run(arguments: args, executable: lsar, password: nil, cancel: cancel)
        if cancel.isCancelled { throw EngineError(message: "cancelled") }
        if out.status != 0 || out.stdout.localizedCaseInsensitiveContains("failed") {
            try failure(out, archive: archive, password: password)
        }
    }

    private static func failure(_ out: SevenZip.Output, archive: URL, password: String?) throws -> Never {
        let text = out.stdout + "\n" + out.stderr
        if text.localizedCaseInsensitiveContains("password") {
            throw PasswordRequired(wrong: password != nil)
        }
        let name = "“\(archive.lastPathComponent)”"
        if text.contains("Couldn't recognize the archive format") || text.contains("not recognized") {
            throw EngineError(message: "\(name) isn't an archive Zipling can open, or it is damaged.")
        }
        let detail = text.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.last { !$0.isEmpty }
        throw EngineError(message: "Couldn't read \(name)" + (detail.map { ": \($0)" } ?? "."))
    }
}

/// Turns unar's one-line-per-file output into progress.
private final class LineCounter: @unchecked Sendable {
    private let total: UInt64
    private let progress: @Sendable (Double) -> Void
    private let lock = NSLock()
    private var done: UInt64 = 0

    init(total: UInt64, progress: @escaping @Sendable (Double) -> Void) {
        self.total = total
        self.progress = progress
    }

    func saw(_ line: String) {
        guard line.hasSuffix("OK."), !line.contains(", rsrc)") else { return }
        lock.lock()
        done += 1
        let fraction = min(1, Double(done) / Double(total))
        lock.unlock()
        progress(fraction)
    }
}
