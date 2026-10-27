import Foundation

/// Changes an archive in place: add, replace, delete, rename, new folder. ZIP, 7z and
/// TAR are updated by 7-Zip directly; Zipling archives and compressed tars are
/// rebuilt with the same engine. Either way the original is only replaced once the
/// new one is complete.
enum Editing {
    enum Change: Sendable {
        /// Files and folders from disk, into a folder of the archive ("" for the top).
        /// Items with the same name are replaced.
        case add([URL], into: String)
        case delete([String])
        case rename(String, to: String)
        case newFolder(String)
    }

    enum Method: Sendable {
        case sevenZip
        /// Extract everything, change it on disk, and make the archive again.
        case rebuild(CreateOptions)
    }

    /// How an archive can be edited, or why it can't.
    static func method(for archive: OpenedArchive, url: URL) -> Result<Method, EngineError> {
        if Archives.volumes(of: url).count > 1 || Archives.isVolume(url.lastPathComponent) {
            return .failure(EngineError(message: "Split archives can't be changed. Extract it and make a new one."))
        }
        if archive.backend == .arca { return .success(.rebuild(CreateOptions(format: .zlg, level: .balanced))) }
        // A .tar.gz opens as its tar, so its format reads "tar.gz".
        switch (archive.listing.format ?? "").lowercased() {
        case "zip", "7z", "tar": return .success(.sevenZip)
        case "tar.gz", "tar.gzip": return .success(.rebuild(CreateOptions(format: .tarGz, leaveOutMacFiles: false)))
        case "tar.xz": return .success(.rebuild(CreateOptions(format: .tarXz, level: .max, leaveOutMacFiles: false)))
        case let format:
            return .failure(EngineError(message: "\(format.uppercased()) archives can't be changed. Extract it and make a ZIP, 7z or Zipling archive instead."))
        }
    }

    static func apply(
        _ change: Change, to archive: OpenedArchive, at url: URL, method: Method, task: EngineTask,
        cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws {
        switch method {
        case .sevenZip:
            // New entries in a 7z get its names hidden too.
            let format = (archive.listing.format ?? "").lowercased()
            try await sevenZip(change, archive: url, password: archive.password, hideNames: format == "7z",
                               zip: format == "zip", cancel: cancel)
        case .rebuild(let options):
            try await rebuild(change, archive: archive, at: url, options: options, task: task, cancel: cancel,
                              progress: progress)
        }
    }

    // MARK: 7-Zip

    private static func sevenZip(
        _ change: Change, archive: URL, password: String?, hideNames: Bool, zip: Bool, cancel: CancelToken
    ) async throws {
        let fm = FileManager.default
        let secret = password == nil ? [] : ["-p"] + (hideNames ? ["-mhe=on"] : [])
        func run(_ command: String, _ options: [String], _ operands: [String], in folder: URL? = nil) async throws {
            let args = [command, "-bso0", "-bse2", "-sccUTF-8", "-scsUTF-8"] + options + secret + ["--", archive.path] + operands
            let out = try await SevenZip.run(arguments: args, executable: try helper(), password: password,
                                             cancel: cancel, workingDirectory: folder)
            try check(out, cancel: cancel)
        }
        switch change {
        case .add(let items, let folder):
            // 7-Zip stores relative paths as given, so the items are laid out under
            // their folder in a scratch folder (APFS clones them: nothing is copied).
            let work = fm.temporaryDirectory.appending(path: "Zipling/edit-\(UUID().uuidString)")
            defer { try? fm.removeItem(at: work) }
            let inside = folder.isEmpty ? work : work.appending(path: folder)
            try fm.createDirectory(at: inside, withIntermediateDirectories: true)
            for item in items { try fm.copyItem(at: item, to: inside.appending(path: item.lastPathComponent)) }
            let relative = items.map { folder.isEmpty ? $0.lastPathComponent : "\(folder)/\($0.lastPathComponent)" }
            // UTF-8 names in ZIPs, so accents survive on Windows.
            try await run("a", ["-snl"] + (zip ? ["-mcu=on"] : []), relative, in: work)
        case .delete(let paths):
            try await run("d", [], paths)
        case .rename(let old, let new):
            try await run("rn", [], [old, new])
        case .newFolder(let path):
            let work = fm.temporaryDirectory.appending(path: "Zipling/edit-\(UUID().uuidString)")
            defer { try? fm.removeItem(at: work) }
            try fm.createDirectory(at: work.appending(path: path), withIntermediateDirectories: true)
            try await run("a", [], [path], in: work)
        }
    }

    private static func helper() throws -> URL {
        guard let exe = SevenZip.executable else {
            throw EngineError(message: "Zipling's 7-Zip helper is missing. Please reinstall Zipling.")
        }
        return exe
    }

    private static func check(_ out: SevenZip.Output, cancel: CancelToken) throws {
        if cancel.isCancelled { throw EngineError(message: "cancelled") }
        guard out.status == 0 else {
            if (out.stderr + out.stdout).contains("Wrong password") { throw PasswordRequired(wrong: true) }
            let detail = (out.stderr + "\n" + out.stdout).split(separator: "\n")
                .map { $0.trimmingCharacters(in: .whitespaces) }.last { !$0.isEmpty }
            throw EngineError(message: "The archive couldn't be changed" + (detail.map { ": \($0)" } ?? "."))
        }
    }

    // MARK: Rebuild

    private static func rebuild(
        _ change: Change, archive: OpenedArchive, at url: URL, options: CreateOptions, task: EngineTask,
        cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws {
        let fm = FileManager.default
        let work = url.deletingLastPathComponent().appending(path: ".zipling-edit-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: work) }
        let tree = work.appending(path: "tree")
        _ = try await Archives.extract(archive, to: tree, only: [], task: task, cancel: cancel) { done, total in
            progress(done / 2, total)
        }
        switch change {
        case .add(let items, let folder):
            let inside = folder.isEmpty ? tree : tree.appending(path: folder)
            try fm.createDirectory(at: inside, withIntermediateDirectories: true)
            for item in items {
                let target = inside.appending(path: item.lastPathComponent)
                if Placement.exists(target) { try fm.removeItem(at: target) }
                try fm.copyItem(at: item, to: target)
            }
        case .delete(let paths):
            for path in paths { try fm.removeItem(at: tree.appending(path: path)) }
        case .rename(let old, let new):
            try fm.moveItem(at: tree.appending(path: old), to: tree.appending(path: new))
        case .newFolder(let path):
            try fm.createDirectory(at: tree.appending(path: path), withIntermediateDirectories: true)
        }
        let top = SafeExtraction.names(in: tree.path).sorted().map { tree.appending(path: $0) }
        guard !top.isEmpty else { throw EngineError(message: "An archive can't be left empty. Move it to the Trash instead.") }
        let made = try await Creation.create(
            top, into: work.appending(path: url.lastPathComponent), options: options, password: nil, task: task,
            cancel: cancel) { done, total in
            progress(total / 2 + done / 2, total)
        }
        guard let rebuilt = made.files.first else { throw EngineError(message: "The archive couldn't be rebuilt.") }
        _ = try fm.replaceItemAt(url, withItemAt: rebuilt)
    }
}
