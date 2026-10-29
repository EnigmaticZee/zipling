import Foundation

/// The kinds of archive Zipling makes.
enum ArchiveFormat: String, CaseIterable, Identifiable, Codable, Sendable {
    case zlg, zip, sevenZip = "7z", tarGz = "tar.gz", tarXz = "tar.xz"
    var id: String { rawValue }

    var title: String {
        switch self {
        case .zlg: "Zipling"
        case .zip: "ZIP"
        case .sevenZip: "7z"
        case .tarGz: "TAR.GZ"
        case .tarXz: "TAR.XZ"
        }
    }

    var fileExtension: String { rawValue }

    var blurb: String {
        switch self {
        case .zlg: "Smallest, with every byte verified. Opens in Zipling."
        case .zip: "Opens anywhere: Windows, Linux, phones, Finder."
        case .sevenZip: "Small, and opens in 7-Zip, Keka and most archivers."
        case .tarGz: "The usual way to share folders with Linux and servers."
        case .tarXz: "Smaller than TAR.GZ, slower to make."
        }
    }

    var supportsPassword: Bool { self == .zip || self == .sevenZip }
    var supportsSplit: Bool { self == .zip || self == .sevenZip }
    /// Whether leaving out .DS_Store and AppleDouble files applies (Zipling keeps Mac
    /// metadata properly instead).
    var canLeaveOutMacFiles: Bool { self != .zlg }
}

/// How to make an archive. Saved presets keep everything but the password.
struct CreateOptions: Codable, Equatable, Sendable {
    var format: ArchiveFormat = .zlg
    var level: Level = .balanced
    var encryptNames = true
    /// Bytes per part, or nil for one file.
    var splitBytes: UInt64?
    var leaveOutMacFiles = true
    /// Zipling archives only: pick the level from a sample of the files.
    var autoLevel: Bool?

    var hasLevels: Bool { format != .tarGz }
    var isAuto: Bool { autoLevel == true && format == .zlg }
}

/// Makes archives: Zipling's own through the Arca engine, the rest through 7-Zip
/// or the system's tar. Everything is written to a hidden folder first and moved
/// into place once it's complete and checked, so a failed or cancelled job leaves
/// nothing behind.
enum Creation {
    struct Made: Sendable {
        let result: PackResult
        /// The archive, or every part of a split one.
        let files: [URL]
    }

    static func create(
        _ items: [URL], into output: URL, options: CreateOptions, password: String?, task: EngineTask,
        cancel: CancelToken, check: Bool = true, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> Made {
        let fm = FileManager.default
        let folder = output.deletingLastPathComponent()
        let staging = folder.appending(path: ".zipling-create-\(UUID().uuidString)")
        try fm.createDirectory(at: staging, withIntermediateDirectories: true)
        defer { try? fm.removeItem(at: staging) }
        let target = staging.appending(path: output.lastPathComponent)
        let started = Date()

        let result: PackResult
        switch options.format {
        case .zlg:
            result = try await Engine.pack(items, into: target, level: options.level, task: task, progress: progress)
        case .zip where engineWritesZip(options, password: password):
            let made = try await Engine.zip(items, into: target, level: engineZipLevel,
                                            leaveOutMacFiles: options.leaveOutMacFiles, task: task, progress: progress)
            result = PackResult(files: made.files, dirs: made.dirs, inputBytes: made.inputBytes,
                                outputBytes: made.outputBytes, dupFileBytes: 0, dupChunkBytes: 0,
                                seconds: made.seconds, transforms: [])
        case .zip, .sevenZip:
            let (files, bytes) = measure(items)
            try await sevenZip(items, into: target, options: options, password: password, totalBytes: bytes,
                               cancel: cancel, progress: progress)
            result = summary(files: files, bytes: bytes, made: try made(at: staging), started: started)
        case .tarGz, .tarXz:
            let (files, bytes) = measure(items)
            try await tar(items, into: target, options: options, totalFiles: files, totalBytes: bytes,
                          cancel: cancel, progress: progress)
            result = summary(files: files, bytes: bytes, made: try made(at: staging), started: started)
        }
        if cancel.isCancelled { throw EngineError(message: "cancelled") }

        // Check what was written before it's handed over (the Arca engine already does).
        if check, options.format != .zlg {
            let first = try made(at: staging).first ?? target
            if options.format == .zip, password?.isEmpty ?? true, await engineVerifiedZip(first, task: task) {
                // Checked by the engine's own reader, ~8x faster than 7-Zip's test.
            } else if options.format == .zip || options.format == .sevenZip {
                try await SevenZip.test(first, password: password, totalBytes: result.inputBytes, cancel: cancel) { _, _ in }
            } else {
                _ = try await SevenZip.list(first, password: nil, cancel: cancel)
            }
        }

        var placed: [URL] = []
        for file in try made(at: staging) {
            let destination = Placement.unique(file.lastPathComponent, in: folder)
            try fm.moveItem(at: file, to: destination)
            placed.append(destination)
        }
        return Made(result: result, files: placed)
    }

    // MARK: Engine ZIP

    /// Fast and Balanced ZIPs without a password or split volumes come from the
    /// engine's writer: on a 660 MB folder, Deflate level 7 on every core made it
    /// 0.07% smaller than 7-Zip's fast Deflate (-mx1) in 1.4 s instead of 1.7 s.
    /// Max and Ultra keep 7-Zip, whose optimal-parsing Deflate is 1% smaller.
    static func engineWritesZip(_ options: CreateOptions, password: String?) -> Bool {
        options.format == .zip && (password?.isEmpty ?? true) && options.splitBytes == nil
            && (options.level == .fast || options.level == .balanced)
    }

    static let engineZipLevel = 7

    /// Whether the engine's ZIP reader checked every entry and found it whole.
    /// Anything else (an archive it doesn't read, or damage) goes to 7-Zip's test,
    /// which has the last word and reports damage in its own words.
    private static func engineVerifiedZip(_ zip: URL, task: EngineTask) async -> Bool {
        do {
            try await Engine.verifyZip(zip, task: task)
            return true
        } catch {
            return false
        }
    }

    // MARK: 7-Zip

    private static func sevenZip(
        _ items: [URL], into target: URL, options: CreateOptions, password: String?, totalBytes: UInt64,
        cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws {
        // Balanced ZIP uses 7-Zip's fast Deflate, as Fast does: level 5's deeper search
        // saved under 2% on a 660 MB folder and took three times as long.
        let level = switch options.level {
        case .fast: 1
        case .balanced: options.format == .zip ? 1 : 5
        case .max: 7
        case .ultra: 9
        }
        var args = ["a", options.format == .zip ? "-tzip" : "-t7z", "-mx=\(level)", "-snl", "-bso0", "-bse2", "-bsp1",
                    "-sccUTF-8", "-scsUTF-8"]
        if options.format == .zip {
            // UTF-8 names, so accents and other scripts survive on Windows.
            args.append("-mcu=on")
        }
        if options.format == .sevenZip, options.level == .balanced {
            // Level 5's 32 MB window, split into 32 MB pieces instead of 128 MB so every
            // core has one: 1.3x faster for about 3% more on a 660 MB folder.
            args.append("-m0=LZMA2:d=32m:c=32m")
        }
        if let password, !password.isEmpty {
            args.append("-p")
            if options.format == .zip { args.append("-mem=AES256") }
            if options.format == .sevenZip, options.encryptNames { args.append("-mhe=on") }
        }
        if let split = options.splitBytes { args.append("-v\(split)b") }
        if options.leaveOutMacFiles {
            args += ["-xr!.DS_Store", "-xr!._*", "-xr!__MACOSX", "-xr!.Spotlight-V100", "-xr!.fseventsd", "-xr!.Trashes"]
        }
        // Absolute paths: 7-Zip stores each item under its own name, not the full path.
        args += ["--", target.path] + items.map { $0.standardizedFileURL.path }
        let out = try await SevenZip.run(args, password: password?.isEmpty == false ? password : nil, cancel: cancel) { fraction in
            progress(UInt64(Double(totalBytes) * fraction), totalBytes)
        }
        if cancel.isCancelled { throw EngineError(message: "cancelled") }
        guard out.status == 0 else {
            let detail = (out.stderr + "\n" + out.stdout).split(separator: "\n")
                .map { $0.trimmingCharacters(in: .whitespaces) }.last { !$0.isEmpty }
            throw EngineError(message: "7-Zip couldn't make the archive" + (detail.map { ": \($0)" } ?? "."))
        }
    }

    // MARK: tar

    private static func tar(
        _ items: [URL], into target: URL, options: CreateOptions, totalFiles: UInt64, totalBytes: UInt64,
        cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws {
        var args = ["-c", options.format == .tarGz ? "-z" : "-J", "-v", "-f", target.path]
        if options.leaveOutMacFiles {
            // No AppleDouble "._" copies or extended attributes: clean on Linux.
            args += ["--no-mac-metadata", "--no-xattrs", "--exclude", ".DS_Store", "--exclude", "._*"]
        }
        if options.format == .tarXz, options.level == .ultra || options.level == .max {
            args += ["--options", "xz:compression-level=9"]
        }
        // Each item relative to its own folder, so the archive holds just their names.
        for item in items {
            args += ["-C", item.deletingLastPathComponent().path, item.lastPathComponent]
        }
        let counter = FileCounter(totalFiles: max(totalFiles, 1), totalBytes: totalBytes, progress: progress)
        // bsdtar lists each file ("a name") on stderr as it goes.
        let out = try await SevenZip.run(arguments: args, executable: URL(filePath: "/usr/bin/tar"), password: nil,
                                         cancel: cancel, stderrLines: { counter.saw($0) })
        if cancel.isCancelled { throw EngineError(message: "cancelled") }
        guard out.status == 0 else {
            let detail = out.stderr.split(separator: "\n").last { !$0.hasPrefix("a ") }
            throw EngineError(message: "Couldn't make the archive" + (detail.map { ": \($0)" } ?? "."))
        }
    }

    // MARK: Helpers

    private static func made(at staging: URL) throws -> [URL] {
        try FileManager.default.contentsOfDirectory(atPath: staging.path).sorted().map { staging.appending(path: $0) }
    }

    private static func summary(files: UInt64, bytes: UInt64, made: [URL], started: Date) -> PackResult {
        let output = made.reduce(UInt64(0)) { total, url in
            total + UInt64((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        }
        return PackResult(files: files, dirs: 0, inputBytes: bytes, outputBytes: output, dupFileBytes: 0,
                          dupChunkBytes: 0, seconds: Date().timeIntervalSince(started), transforms: [])
    }

    /// Files and bytes under the items.
    static func measure(_ items: [URL]) -> (UInt64, UInt64) {
        var files: UInt64 = 0, bytes: UInt64 = 0
        let keys: Set<URLResourceKey> = [.fileSizeKey, .isRegularFileKey]
        for url in items {
            if let v = try? url.resourceValues(forKeys: keys), v.isRegularFile == true {
                files += 1
                bytes += UInt64(v.fileSize ?? 0)
                continue
            }
            let walker = FileManager.default.enumerator(at: url, includingPropertiesForKeys: Array(keys))
            while let file = walker?.nextObject() as? URL {
                if let v = try? file.resourceValues(forKeys: keys), v.isRegularFile == true {
                    files += 1
                    bytes += UInt64(v.fileSize ?? 0)
                }
            }
        }
        return (files, bytes)
    }
}

/// Turns tar's file-by-file listing into progress.
private final class FileCounter: @unchecked Sendable {
    private let totalFiles: UInt64, totalBytes: UInt64
    private let progress: @Sendable (UInt64, UInt64) -> Void
    private let lock = NSLock()
    private var done: UInt64 = 0

    init(totalFiles: UInt64, totalBytes: UInt64, progress: @escaping @Sendable (UInt64, UInt64) -> Void) {
        self.totalFiles = totalFiles
        self.totalBytes = totalBytes
        self.progress = progress
    }

    func saw(_ line: String) {
        guard line.hasPrefix("a ") else { return }
        lock.lock()
        done += 1
        let fraction = min(1, Double(done) / Double(totalFiles))
        lock.unlock()
        progress(UInt64(Double(totalBytes) * fraction), totalBytes)
    }
}
