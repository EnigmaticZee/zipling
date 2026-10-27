import AppIntents
import Foundation

/// Shortcuts actions: compress, extract and convert, without opening a window.

enum ArchiveFormatChoice: String, AppEnum {
    case zlg, zip, sevenZip, tarGz, tarXz

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Archive Format"
    static let caseDisplayRepresentations: [ArchiveFormatChoice: DisplayRepresentation] = [
        .zlg: "Zipling (.zlg)", .zip: "ZIP", .sevenZip: "7z", .tarGz: "TAR.GZ", .tarXz: "TAR.XZ",
    ]

    var format: ArchiveFormat {
        switch self {
        case .zlg: .zlg
        case .zip: .zip
        case .sevenZip: .sevenZip
        case .tarGz: .tarGz
        case .tarXz: .tarXz
        }
    }
}

enum LevelChoice: String, AppEnum {
    case auto, fast, balanced, max, ultra

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Compression"
    static let caseDisplayRepresentations: [LevelChoice: DisplayRepresentation] = [
        .auto: "Auto", .fast: "Fast", .balanced: "Balanced", .max: "Max", .ultra: "Ultra",
    ]

    var level: Level? { Level(rawValue: rawValue) }
}

/// Files handed over by Shortcuts, as paths on disk: files that only arrive as data
/// are written to a private folder first.
@MainActor
private func urls(for files: [IntentFile]) throws -> [URL] {
    let folder = FileManager.default.temporaryDirectory.appending(path: "Zipling/shortcuts-\(UUID().uuidString)")
    return try files.map { file in
        if let url = file.fileURL { return url }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appending(path: file.filename.isEmpty ? "File" : file.filename)
        try file.data.write(to: url)
        return url
    }
}

struct CompressFilesIntent: AppIntent {
    static let title: LocalizedStringResource = "Compress Files"
    static let description = IntentDescription("Makes an archive of files and folders with Zipling, next to the first one.")

    @Parameter(title: "Files", supportedContentTypes: [.item]) var files: [IntentFile]
    @Parameter(title: "Format", default: .zlg) var format: ArchiveFormatChoice
    @Parameter(title: "Compression", default: .auto) var level: LevelChoice

    static var parameterSummary: some ParameterSummary {
        Summary("Compress \(\.$files) as \(\.$format)") { \.$level }
    }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<IntentFile> {
        let archive = try await Automation.compress(try urls(for: files), format: format.format, level: level.level)
        return .result(value: IntentFile(fileURL: archive))
    }
}

struct ExtractArchiveIntent: AppIntent {
    static let title: LocalizedStringResource = "Extract Archive"
    static let description = IntentDescription("Extracts an archive next to itself: one item comes out as it is, several in a folder named after the archive.")

    @Parameter(title: "Archive", supportedContentTypes: [.archive, .data]) var archive: IntentFile
    @Parameter(title: "Password") var password: String?

    static var parameterSummary: some ParameterSummary {
        Summary("Extract \(\.$archive)") { \.$password }
    }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<[IntentFile]> {
        let placed = try await Automation.extract(try urls(for: [archive])[0], password: password)
        return .result(value: placed.map { IntentFile(fileURL: $0) })
    }
}

struct ConvertArchiveIntent: AppIntent {
    static let title: LocalizedStringResource = "Convert Archive"
    static let description = IntentDescription("Makes the same files into another format next to the original: a smaller Zipling archive, or a ZIP or 7z for anyone.")

    @Parameter(title: "Archive", supportedContentTypes: [.archive, .data]) var archive: IntentFile
    @Parameter(title: "Format", default: .zlg) var format: ArchiveFormatChoice
    @Parameter(title: "Compression", default: .balanced) var level: LevelChoice
    @Parameter(title: "Password") var password: String?

    static var parameterSummary: some ParameterSummary {
        Summary("Convert \(\.$archive) to \(\.$format)") {
            \.$level
            \.$password
        }
    }

    @MainActor
    func perform() async throws -> some IntentResult & ReturnsValue<IntentFile> {
        let made = try await Automation.convert(try urls(for: [archive])[0], to: format.format,
                                                level: level.level ?? .balanced, password: password)
        return .result(value: IntentFile(fileURL: made))
    }
}

struct ZiplingShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: CompressFilesIntent(), phrases: ["Compress files with \(.applicationName)"],
                    shortTitle: "Compress Files", systemImageName: "archivebox")
        AppShortcut(intent: ExtractArchiveIntent(), phrases: ["Extract an archive with \(.applicationName)"],
                    shortTitle: "Extract Archive", systemImageName: "arrow.down.doc")
        AppShortcut(intent: ConvertArchiveIntent(), phrases: ["Convert an archive with \(.applicationName)"],
                    shortTitle: "Convert Archive", systemImageName: "arrow.triangle.2.circlepath")
    }
}
