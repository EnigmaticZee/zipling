import Foundation

/// Turns an archive into another format with the same contents: a much smaller
/// Zipling archive, or a tighter ZIP or 7z that still opens anywhere. The archive is
/// extracted into a hidden folder next to it, made again in the new format and read
/// back; nothing is placed until that has worked.
enum Conversion {
    struct Result: Sendable {
        let made: Creation.Made
        /// The original archive's size on disk.
        let before: UInt64
        var after: UInt64 { made.files.reduce(0) { $0 + Conversion.size(of: $1) } }
    }

    static func convert(
        _ archive: OpenedArchive, from original: URL, options: CreateOptions, password: String?, quarantine: Data?,
        task: EngineTask, cancel: CancelToken, progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> Result {
        let fm = FileManager.default
        let folder = original.deletingLastPathComponent()
        let work = folder.appending(path: ".zipling-convert-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: work) }
        let contents = work.appending(path: "contents")

        // First half of the progress bar: getting everything out.
        _ = try await Archives.extract(archive, to: contents, only: [], task: task, cancel: cancel) { done, total in
            progress(done / 2, total)
        }
        _ = try SafeExtraction.finalize(contents, quarantine: quarantine)
        let items = SafeExtraction.names(in: contents.path).sorted().map { contents.appending(path: $0) }
        guard !items.isEmpty else { throw EngineError(message: "“\(original.lastPathComponent)” is empty.") }

        // Second half: making the new archive. Everything is kept, Mac metadata included.
        var options = options
        options.leaveOutMacFiles = false
        let name = Archives.baseName(original.lastPathComponent) + "." + options.format.fileExtension
        let made = try await Creation.create(
            items, into: folder.appending(path: name), options: options, password: password, task: task,
            cancel: cancel) { done, total in
            progress(total / 2 + done / 2, total)
        }
        let before = Archives.volumes(of: original).reduce(0) { $0 + size(of: $1) }
        return Result(made: made, before: before)
    }

    static func size(of url: URL) -> UInt64 {
        UInt64((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
    }
}
