import Foundation

/// An archive's table of contents, as reported by `arca_list`.
struct Listing: Decodable, Sendable {
    let archiveBytes: UInt64
    let created: Int64
    let files: UInt64
    let dirs: UInt64
    let totalSize: UInt64
    let blocks: Int
    let entries: [ArchiveEntry]

    var createdDate: Date { Date(timeIntervalSince1970: TimeInterval(created)) }

    /// Fraction of the original size saved (0.68 = 68% smaller).
    var saved: Double {
        totalSize == 0 ? 0 : max(0, 1 - Double(archiveBytes) / Double(totalSize))
    }
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
}
