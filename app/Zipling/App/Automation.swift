import AppKit

/// Compress, extract and convert without a window, for Shortcuts, AppleScript and
/// Folder Actions. Each one shows in the app's activity list like any other job.
@MainActor
enum Automation {
    /// Makes an archive next to the first item (or in `folder`). A nil level means
    /// Auto for Zipling archives and Balanced for the others.
    static func compress(
        _ items: [URL], format: ArchiveFormat = .zlg, level: Level? = nil, password: String? = nil,
        into folder: URL? = nil
    ) async throws -> URL {
        guard let first = items.first else { throw EngineError(message: "There's nothing to compress.") }
        var options = CreateOptions(format: format, level: level ?? .balanced)
        if level == nil, format == .zlg {
            let contents = await ContentSummary.measure(items)
            options.level = (try? await Estimate.auto(items, contents: contents, task: EngineTask(), cancel: CancelToken()))?
                .level ?? .balanced
        }
        let base = items.count == 1 ? first.deletingPathExtension().lastPathComponent : "Archive"
        let output = Placement.unique("\(base).\(format.fileExtension)", in: folder ?? first.deletingLastPathComponent())
        let job = Job(title: "Compressing as \(format.title)…")
        JobCenter.shared.track(job)
        do {
            let made = try await Creation.create(items, into: output, options: options,
                                                 password: format.supportsPassword ? password : nil,
                                                 task: job.task, cancel: job.cancelToken,
                                                 check: Preferences.checkNewArchives, progress: job.progressHandler)
            job.finish(nil, outcome: "Made \(made.files.map(\.lastPathComponent).joined(separator: ", ")).", placed: made.files)
            AppRouter.shared.noteCreated(made.files[0])
            return made.files[0]
        } catch {
            job.finish(error)
            throw error
        }
    }

    /// Extract Here, or into `folder`: one item at the top comes out as it is, several
    /// in a folder named after the archive. Names already taken get a number.
    static func extract(_ archive: URL, into folder: URL? = nil, password: String? = nil) async throws -> [URL] {
        let job = Job(title: "Extracting “\(archive.lastPathComponent)”…")
        JobCenter.shared.track(job)
        let scratch = FileManager.default.temporaryDirectory.appending(path: "Zipling/\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: scratch) }
        do {
            let opened = try await Archives.open(archive, password: password, scratch: scratch, cancel: job.cancelToken)
            let outcome = try await Extraction.run(
                opened, only: [], to: folder ?? archive.deletingLastPathComponent(),
                layout: .smart(folder: Archives.baseName(archive.lastPathComponent)), job: job,
                quarantine: SafeExtraction.quarantine(of: archive), conflicts: Conflicts(policy: .keepBoth))
            job.finish(nil, outcome: outcome.summary, placed: outcome.placed)
            return outcome.placed
        } catch let p as PasswordRequired {
            let error = EngineError(message: p.wrong ? "That password didn't work for “\(archive.lastPathComponent)”."
                                    : "“\(archive.lastPathComponent)” is encrypted: give its password.")
            job.finish(error)
            throw error
        } catch {
            job.finish(error)
            throw error
        }
    }

    /// The same files in another format, next to the original.
    static func convert(
        _ archive: URL, to format: ArchiveFormat, level: Level = .balanced, password: String? = nil
    ) async throws -> URL {
        let job = Job(title: "Converting to \(format.title)…")
        JobCenter.shared.track(job)
        let scratch = FileManager.default.temporaryDirectory.appending(path: "Zipling/\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: scratch) }
        do {
            let opened = try await Archives.open(archive, password: password, scratch: scratch, cancel: job.cancelToken)
            let result = try await Conversion.convert(
                opened, from: archive, options: CreateOptions(format: format, level: level),
                password: format.supportsPassword ? password : nil, quarantine: SafeExtraction.quarantine(of: archive),
                task: job.task, cancel: job.cancelToken, progress: job.progressHandler)
            job.finish(nil, outcome: "Made \(result.made.files[0].lastPathComponent): \(Format.bytes(result.before)) → \(Format.bytes(result.after)).",
                       placed: result.made.files)
            return result.made.files[0]
        } catch {
            job.finish(error)
            throw error
        }
    }

    // MARK: Folder Actions

    /// Folder Action scripts that hand added items to Zipling, installed where Folder
    /// Actions Setup lists them.
    static let folderActions: [(name: String, source: String)] = [
        ("Zipling – Extract Added Archives", """
        -- Extracts archives added to this folder, each next to itself, with Zipling.
        on adding folder items to theFolder after receiving theItems
        	repeat with anItem in theItems
        		set itemInfo to info for anItem
        		if name of itemInfo does not start with "." and name extension of itemInfo is in {"zlg", "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "dmg", "iso", "sit", "sitx", "cab", "lzh", "arj"} then
        			tell application id "com.observantsystems.zipling" to extract anItem
        		end if
        	end repeat
        end adding folder items to
        """),
        ("Zipling – Compress Added Items", """
        -- Compresses each item added to this folder into a Zipling archive beside it.
        on adding folder items to theFolder after receiving theItems
        	repeat with anItem in theItems
        		set itemInfo to info for anItem
        		if name of itemInfo does not start with "." and name extension of itemInfo is not in {"zlg", "zip", "7z", "rar", "tar", "gz", "tgz", "bz2", "xz", "zst", "dmg", "iso", "recovery"} then
        			tell application id "com.observantsystems.zipling" to compress {anItem}
        		end if
        	end repeat
        end adding folder items to
        """),
    ]

    /// Compiles the Folder Action scripts into ~/Library/Scripts/Folder Action Scripts.
    static func installFolderActions(
        into folder: URL = FileManager.default.homeDirectoryForCurrentUser.appending(path: "Library/Scripts/Folder Action Scripts")
    ) throws -> URL {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        for (name, source) in folderActions {
            let text = FileManager.default.temporaryDirectory.appending(path: "zipling-\(UUID().uuidString).applescript")
            try source.write(to: text, atomically: true, encoding: .utf8)
            defer { try? FileManager.default.removeItem(at: text) }
            let p = Process()
            p.executableURL = URL(filePath: "/usr/bin/osacompile")
            p.arguments = ["-o", folder.appending(path: "\(name).scpt").path, text.path]
            let err = Pipe()
            p.standardError = err
            try p.run()
            p.waitUntilExit()
            guard p.terminationStatus == 0 else {
                let detail = String(decoding: err.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
                throw EngineError(message: "Couldn't install “\(name)”: \(detail)")
            }
        }
        return folder
    }
}
