import Foundation

/// An archive's table of contents: from `arca_list` for Zipling archives, or from the
/// 7-Zip helper for every other format (which also fills the optional fields).
struct Listing: Decodable, Sendable {
    let archiveBytes: UInt64
    let created: Int64
    let files: UInt64
    let dirs: UInt64
    let totalSize: UInt64
    let blocks: Int
    let entries: [ArchiveEntry]
    /// The format as 7-Zip names it ("zip", "7z", "Rar5"…); nil for Zipling archives.
    var format: String?
    /// The archive-wide method, such as "LZMA2:24" (7-Zip formats only).
    var method: String?
    var solid: Bool?
    /// True when some entries (or the names themselves) are encrypted.
    var encrypted: Bool?
    /// Mac metadata stored beside the files (`__MACOSX/…/._name`), in archive order.
    /// Not listed as entries: it's applied to the files it describes on extraction.
    var macMetadata: [MacMetadata]?

    var isZipling: Bool { format == nil }
    var createdDate: Date { Date(timeIntervalSince1970: TimeInterval(created)) }

    /// Fraction of the original size saved (0.68 = 68% smaller).
    var saved: Double {
        totalSize == 0 ? 0 : max(0, 1 - Double(archiveBytes) / Double(totalSize))
    }
}

/// One AppleDouble file in an archive: its path as stored, and its size.
struct MacMetadata: Decodable, Sendable, Hashable {
    let path: String
    let size: UInt64
}

struct ArchiveEntry: Decodable, Sendable, Hashable {
    let path: String
    let kind: String
    let size: UInt64
    let mtime: Int64
    let mode: UInt32
    let transform: String
    let codec: String
    let packed: UInt64
    let target: String?
    /// False when the format doesn't record a per-file packed size (solid 7z, tar).
    var packedKnown: Bool?
    var encrypted: Bool?
    /// The entry's position as The Unarchiver numbers it (used to extract by index).
    var index: Int?
    /// The index of its resource fork, which The Unarchiver lists as a separate entry.
    var forkIndex: Int?

    var isDirectory: Bool { kind == "dir" }
    var name: String { path.split(separator: "/").last.map(String.init) ?? path }
    var parent: String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        return String(path[..<slash])
    }
}

struct PackResult: Decodable, Sendable {
    let files: UInt64
    let dirs: UInt64
    let inputBytes: UInt64
    let outputBytes: UInt64
    let dupFileBytes: UInt64
    let dupChunkBytes: UInt64
    let seconds: Double
    let transforms: [TransformStat]

    var saved: Double { inputBytes == 0 ? 0 : max(0, 1 - Double(outputBytes) / Double(inputBytes)) }
}

/// One reversible transform's totals: `[name, files, bytes in, bytes out]` on the wire.
struct TransformStat: Decodable, Sendable {
    let name: String
    let files: UInt64
    let input: UInt64
    let output: UInt64

    init(from decoder: Decoder) throws {
        var c = try decoder.unkeyedContainer()
        name = try c.decode(String.self)
        files = try c.decode(UInt64.self)
        input = try c.decode(UInt64.self)
        output = try c.decode(UInt64.self)
    }
}

struct ExtractResult: Decodable, Sendable {
    let files: UInt64
    let bytes: UInt64
    let seconds: Double
    /// Entries left out because writing them would have been unsafe.
    var skipped: [String]?
}
