import Foundation
import os

/// Cancellation that every backend understands: helper processes register a
/// handler that terminates them. (The Arca engine has its own `EngineTask`.)
final class CancelToken: Sendable {
    private let state = OSAllocatedUnfairLock<(cancelled: Bool, handlers: [@Sendable () -> Void])>(
        initialState: (false, []))

    var isCancelled: Bool { state.withLock { $0.cancelled } }

    func cancel() {
        let handlers = state.withLock { s -> [@Sendable () -> Void] in
            guard !s.cancelled else { return [] }
            s.cancelled = true
            defer { s.handlers = [] }
            return s.handlers
        }
        handlers.forEach { $0() }
    }

    /// Runs `handler` when cancelled, or right away if that already happened.
    func onCancel(_ handler: @escaping @Sendable () -> Void) {
        let already = state.withLock { s -> Bool in
            if !s.cancelled { s.handlers.append(handler) }
            return s.cancelled
        }
        if already { handler() }
    }
}

/// An archive needs a password before it can be listed or extracted.
struct PasswordRequired: Error {
    /// True when a password was given and it was wrong.
    let wrong: Bool
}

/// The 7-Zip command-line helper (`7zz`), run unmodified as a separate process. It
/// reads ZIP, 7z, RAR, TAR, DMG, ISO and about 60 other formats. Running it out of
/// process means a damaged or hostile archive can crash the helper, never Zipling.
enum SevenZip {
    /// The helper inside the app bundle. Debug builds fall back to a Homebrew install.
    static var executable: URL? {
        let fm = FileManager.default
        let bundled = Bundle.main.bundleURL.appending(path: "Contents/Helpers/7zz")
        if fm.isExecutableFile(atPath: bundled.path) { return bundled }
        #if DEBUG
        for path in ["/opt/homebrew/bin/7zz", "/usr/local/bin/7zz"] where fm.isExecutableFile(atPath: path) {
            return URL(filePath: path)
        }
        #endif
        return nil
    }

    // MARK: Listing

    static func list(_ archive: URL, password: String?, cancel: CancelToken? = nil) async throws -> Listing {
        let out = try await run(["l", "-slt", "-sccUTF-8", archive.path], password: password, cancel: cancel)
        try check(out, archive: archive, password: password, cancel: cancel)
        return parseListing(out.stdout, archive: archive)
    }

    /// Parses `7zz l -slt`: one "--" header block per archive layer (a DMG holds an
    /// HFS+ or APFS volume, which 7-Zip opens too), then after "----------" one
    /// blank-line-separated block of "Key = Value" lines per entry.
    static func parseListing(_ text: String, archive: URL) -> Listing {
        var headers: [[String: String]] = []
        var blocks: [[String: String]] = []
        var current: [String: String] = [:]
        var section = 0  // 0 before the first "--", 1 a layer's header, 2 entries
        var skipping = false  // the "----" partition block between two layers
        for raw in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = String(raw)
            if section < 2 && line == "--" {
                section = 1
                skipping = false
                headers.append([:])
                continue
            }
            if section < 2 && line == "----" { skipping = true; continue }
            if line == "----------" { section = 2; continue }
            guard section > 0, !skipping else { continue }
            if line.isEmpty {
                if section == 2, !current.isEmpty { blocks.append(current) }
                current = [:]
                continue
            }
            guard let eq = line.range(of: " = ") else {
                // Keys with empty values print as "Key = " and keep their trailing space.
                if section == 2, line.hasSuffix(" =") { current[String(line.dropLast(2))] = "" }
                continue
            }
            let key = String(line[..<eq.lowerBound]), value = String(line[eq.upperBound...])
            if section == 1 { headers[headers.count - 1][key] = value } else { current[key] = value }
        }
        if section == 2, !current.isEmpty { blocks.append(current) }
        let outer = headers.first ?? [:], inner = headers.last ?? [:]
        let types = Set(headers.compactMap { $0["Type"]?.uppercased() })
        let hidden: Set<String> = types.isDisjoint(with: ["HFS", "APFS"]) ? []
            : ["[HFS+ Private Data]", ".HFS+ Private Directory Data", ".fseventsd", ".Trashes", ".journal", ".journal_info_block"]

        var entries: [ArchiveEntry] = []
        var files: UInt64 = 0, dirs: UInt64 = 0, total: UInt64 = 0
        var anyEncrypted = headers.contains { $0["Encrypted"] == "+" }
        let isStream = Archives.streamFormats.contains((outer["Type"] ?? "").lowercased())
        for b in blocks {
            // xz and bzip2 don't store the name of what they compress; 7-Zip leaves it
            // blank and names the extracted file after the archive.
            let raw = (b["Path"] ?? "").isEmpty && isStream ? streamName(archive) : (b["Path"] ?? "")
            let path = cleanPath(raw)
            guard !path.isEmpty, !isAppleDouble(path) else { continue }
            if !hidden.isEmpty, path.split(separator: "/").contains(where: {
                hidden.contains(String($0)) || $0.contains("HFS+ Private")
            }) { continue }
            let mode = permissions(b["Mode"] ?? b["Attributes"] ?? "")
            let isDir = b["Folder"] == "+" || mode.type == "d" || (b["Attributes"] ?? "").hasPrefix("D")
            let link = b["Symbolic Link"].flatMap { $0.isEmpty ? nil : $0 }
            let kind = isDir ? "dir" : (link != nil || mode.type == "l") ? "link" : "file"
            let size = UInt64(b["Size"] ?? "") ?? 0
            let packedText = b["Packed Size"] ?? ""
            let encrypted = b["Encrypted"] == "+"
            anyEncrypted = anyEncrypted || encrypted
            if kind == "dir" { dirs += 1 } else { files += 1; total += size }
            entries.append(ArchiveEntry(
                path: path,
                kind: kind, size: size, mtime: timestamp(b["Modified"]), mode: mode.bits,
                transform: "foreign", codec: b["Method"] ?? "", packed: UInt64(packedText) ?? 0,
                target: link, packedKnown: !packedText.isEmpty && kind == "file", encrypted: encrypted))
        }
        let attributes = try? FileManager.default.attributesOfItem(atPath: archive.path)
        let fileBytes = (attributes?[.size] as? NSNumber)?.uint64Value ?? 0
        let created = (attributes?[.modificationDate] as? Date) ?? .now
        return Listing(
            archiveBytes: UInt64(outer["Physical Size"] ?? "") ?? fileBytes,
            created: Int64(created.timeIntervalSince1970), files: files, dirs: dirs, totalSize: total,
            blocks: Int(inner["Blocks"] ?? "") ?? 0, entries: entries,
            format: outer["Type"] ?? "archive", method: inner["Method"].flatMap { $0.isEmpty ? nil : $0 },
            solid: inner["Solid"].map { $0 == "+" }, encrypted: anyEncrypted)
    }

    /// The path as 7-Zip writes it: "..", "." and empty components dropped, so
    /// "../evil.txt" and "/tmp/x" land inside the destination as "evil.txt" and "tmp/x".
    static func cleanPath(_ path: String) -> String {
        path.split(separator: "/").filter { $0 != ".." && $0 != "." }.joined(separator: "/")
    }

    /// The name 7-Zip gives the contents of a single compressed stream.
    static func streamName(_ archive: URL) -> String {
        let base = archive.deletingPathExtension().lastPathComponent
        let tarry: Set<String> = ["tgz", "taz", "tbz", "tbz2", "txz", "tzst", "tlz4", "tlz"]
        return tarry.contains(archive.pathExtension.lowercased()) ? base + ".tar" : base
    }

    /// Mac metadata that Finder and `ditto` store beside files (`__MACOSX/`, `._name`).
    /// It is merged back into the real files on extraction, so it is never listed.
    static func isAppleDouble(_ path: String) -> Bool {
        let parts = path.split(separator: "/")
        return parts.contains("__MACOSX") || (parts.last?.hasPrefix("._") ?? false)
    }

    /// "A -rw-r--r--", "D drwxr-xr-x" or "lrwxr-xr-x" → file type letter and permission bits.
    static func permissions(_ field: String) -> (type: Character?, bits: UInt32) {
        guard let token = field.split(separator: " ").last, token.count == 10 else { return (nil, 0o644) }
        let chars = Array(token)
        var bits: UInt32 = 0
        for (i, c) in chars.dropFirst().enumerated() where c != "-" {
            bits |= 1 << UInt32(8 - i)
        }
        return (chars[0], bits)
    }

    private static let dateParser: DateFormatter = {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.dateFormat = "yyyy-MM-dd HH:mm:ss"
        return f
    }()

    /// 7-Zip prints local time as "2026-10-26 18:39:52.6631284".
    static func timestamp(_ text: String?) -> Int64 {
        guard let text, text.count >= 19, let date = dateParser.date(from: String(text.prefix(19))) else { return 0 }
        return Int64(date.timeIntervalSince1970)
    }

    // MARK: Extracting and testing

    /// Extracts everything, or just the given paths (folders include their contents).
    /// Returns the entries 7-Zip refused to write because they were unsafe (a path
    /// through a symbolic link, for example); everything else is extracted.
    @discardableResult
    static func extract(
        _ archive: URL, to dest: URL, only: [String], password: String?, totalBytes: UInt64,
        cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> [String] {
        try FileManager.default.createDirectory(at: dest, withIntermediateDirectories: true)
        var args = ["x", "-y", "-snl", "-spd", "-bso0", "-bse2", "-bsp1", "-sccUTF-8", "-scsUTF-8",
                    "-o" + dest.path, archive.path]
        var listFile: URL?
        if !only.isEmpty {
            // A list file instead of arguments: no length limit, and with -spd no wildcards.
            let file = FileManager.default.temporaryDirectory.appending(path: "zipling-list-\(UUID().uuidString).txt")
            try only.joined(separator: "\n").write(to: file, atomically: true, encoding: .utf8)
            args.append("@" + file.path)
            listFile = file
        }
        defer { if let listFile { try? FileManager.default.removeItem(at: listFile) } }
        let out = try await run(args, password: password, cancel: cancel) { fraction in
            progress(UInt64(Double(totalBytes) * fraction), totalBytes)
        }
        let skipped = unsafeEntries(out, under: dest)
        if skipped.isEmpty || cancel.isCancelled {
            try check(out, archive: archive, password: password, cancel: cancel)
        }
        progress(totalBytes, totalBytes)
        return skipped
    }

    /// When every error is an entry 7-Zip refused to write for safety, the extraction
    /// counts as done, minus those entries. Any other error (a checksum, a password)
    /// still fails the whole extraction.
    static func unsafeEntries(_ out: Output, under dest: URL) -> [String] {
        guard out.status != 0 else { return [] }
        let errors = out.stderr.split(separator: "\n").map(String.init).filter { $0.hasPrefix("ERROR:") }
        let safetySkips = ["Cannot open output file", "Dangerous link path", "Dangerous symbolic link",
                           "Cannot create symbolic link"]
        guard !errors.isEmpty, errors.allSatisfy({ e in safetySkips.contains { e.contains($0) } }) else { return [] }
        let prefix = dest.path + "/"
        return errors.compactMap { line in
            guard let path = line.components(separatedBy: " : ").last else { return nil }
            return path.hasPrefix(prefix) ? String(path.dropFirst(prefix.count)) : path
        }
    }

    /// Decodes everything and checks every checksum, writing nothing.
    static func test(
        _ archive: URL, password: String?, totalBytes: UInt64,
        cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws {
        let out = try await run(["t", "-bso0", "-bse2", "-bsp1", archive.path], password: password, cancel: cancel) { fraction in
            progress(UInt64(Double(totalBytes) * fraction), totalBytes)
        }
        try check(out, archive: archive, password: password, cancel: cancel)
    }

    // MARK: Running the helper

    struct Output: Sendable {
        let status: Int32
        let stdout: String
        let stderr: String
    }

    /// Turns a failed run into an error people can act on.
    static func check(_ out: Output, archive: URL, password: String?, cancel: CancelToken?) throws {
        if cancel?.isCancelled == true { throw EngineError(message: "cancelled") }
        // The prompt is printed even when the password arrives on stdin, so only a
        // failed run means the password was missing or wrong.
        guard out.status != 0 else { return }
        let text = out.stderr + "\n" + out.stdout
        // With no password, the helper reaches its prompt, finds stdin closed and stops.
        if text.contains("Wrong password") || text.contains("Enter password")
            || (password == nil && text.contains("Break signaled")) {
            throw PasswordRequired(wrong: password != nil)
        }
        let name = "“\(archive.lastPathComponent)”"
        if text.contains("Cannot open the file as archive") || text.contains("Can not open the file as archive") {
            throw EngineError(message: "\(name) isn't an archive Zipling can open, or it is damaged.")
        }
        if text.contains("Unexpected end of archive") || text.contains("Missing volume") {
            throw EngineError(message: "\(name) is incomplete. If it is split into parts, keep all the parts in the same folder.")
        }
        if text.contains("CRC Failed") || text.contains("Data Error") || text.contains("Headers Error") {
            throw EngineError(message: "\(name) is damaged: some of its data doesn't match its checksums.")
        }
        if text.contains("There is not enough space") || text.contains("No space left") {
            throw EngineError(message: "There isn't enough free space to extract \(name).")
        }
        let detail = out.stderr.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty && $0 != "ERROR:" && !$0.hasPrefix("ERROR: /") }
        throw EngineError(message: "7-Zip couldn't read \(name)" + (detail.map { ": \($0)" } ?? "."))
    }

    /// Runs the helper. The password, if any, goes in on stdin, never on the command
    /// line where other processes could read it. With no password, stdin is closed, so
    /// the helper gives up at its password prompt instead of waiting.
    static func run(
        _ arguments: [String], password: String?, cancel: CancelToken?,
        progress: (@Sendable (Double) -> Void)? = nil
    ) async throws -> Output {
        guard let exe = executable else {
            throw EngineError(message: "Zipling's 7-Zip helper is missing. Please reinstall Zipling.")
        }
        let run = HelperRun(executable: exe, arguments: arguments, progress: progress)
        cancel?.onCancel { run.terminate() }
        return try await run.start(password: password)
    }
}

/// One helper process: collects its output, reports progress, and can be terminated
/// from any thread.
private final class HelperRun: @unchecked Sendable {
    private let process = Process()
    private let progress: (@Sendable (Double) -> Void)?
    private let lock = NSLock()
    private var out = Data(), err = Data()
    private var lastFraction = -1.0

    init(executable: URL, arguments: [String], progress: (@Sendable (Double) -> Void)?) {
        process.executableURL = executable
        process.arguments = arguments
        var env = ProcessInfo.processInfo.environment
        env["LC_ALL"] = "en_US.UTF-8"
        env["LANG"] = "en_US.UTF-8"
        process.environment = env
        self.progress = progress
    }

    func terminate() {
        lock.lock()
        defer { lock.unlock() }
        if process.isRunning { process.terminate() }
    }

    func start(password: String?) async throws -> SevenZip.Output {
        let stdout = Pipe(), stderr = Pipe(), stdin = Pipe()
        process.standardOutput = stdout
        process.standardError = stderr
        process.standardInput = stdin
        stdout.fileHandleForReading.readabilityHandler = { [weak self] h in self?.append(h.availableData, toStdout: true) }
        stderr.fileHandleForReading.readabilityHandler = { [weak self] h in self?.append(h.availableData, toStdout: false) }

        return try await withCheckedThrowingContinuation { continuation in
            process.terminationHandler = { [self] p in
                stdout.fileHandleForReading.readabilityHandler = nil
                stderr.fileHandleForReading.readabilityHandler = nil
                append(stdout.fileHandleForReading.readDataToEndOfFile(), toStdout: true)
                append(stderr.fileHandleForReading.readDataToEndOfFile(), toStdout: false)
                lock.lock()
                let result = SevenZip.Output(status: p.terminationStatus,
                                             stdout: String(decoding: out, as: UTF8.self),
                                             stderr: String(decoding: err, as: UTF8.self))
                lock.unlock()
                continuation.resume(returning: result)
            }
            do {
                lock.lock()
                defer { lock.unlock() }
                try process.run()
            } catch {
                continuation.resume(throwing: EngineError(message: "Couldn't start the 7-Zip helper: \(error.localizedDescription)"))
                return
            }
            let input = stdin.fileHandleForWriting
            if let password { try? input.write(contentsOf: Data((password + "\n").utf8)) }
            try? input.close()
        }
    }

    private func append(_ data: Data, toStdout: Bool) {
        guard !data.isEmpty else { return }
        lock.lock()
        if toStdout { out.append(data) } else { err.append(data) }
        // -bsp1 redraws "  42%" in place with backspaces; the latest percentage wins.
        var fraction: Double?
        if toStdout, progress != nil, let text = String(data: data, encoding: .utf8) {
            for token in text.split(whereSeparator: { $0 == "\u{8}" || $0 == "\r" || $0 == "\n" }) {
                let t = token.trimmingCharacters(in: .whitespaces)
                if let pct = t.firstIndex(of: "%"), let value = Double(t[..<pct]) {
                    fraction = min(1, value / 100)
                }
            }
        }
        let report = fraction.flatMap { $0 > lastFraction ? $0 : nil }
        if let report { lastFraction = report }
        lock.unlock()
        if let report { progress?(report) }
    }
}
