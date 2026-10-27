import Foundation
import QuickLookUI
import UniformTypeIdentifiers

/// Finder's Quick Look (space bar) for archives: what's inside, how big it is, and
/// how much smaller the archive is, without opening Zipling.
final class PreviewProvider: QLPreviewProvider, QLPreviewingController {
    func providePreview(for request: QLFilePreviewRequest) async throws -> QLPreviewReply {
        let url = request.fileURL
        let peek = try await Peek.read(url)
        let html = PreviewPage.html(name: url.lastPathComponent, peek: peek)
        return QLPreviewReply(dataOfContentType: .html, contentSize: CGSize(width: 760, height: 560)) { _ in
            Data(html.utf8)
        }
    }
}

/// An archive's contents, enough for a preview.
struct Peek {
    struct Entry {
        let path: String
        let isFolder: Bool
        let size: UInt64
        let modified: Date?
        let encrypted: Bool
    }

    let format: String
    let entries: [Entry]
    let archiveBytes: UInt64
    let totalSize: UInt64

    static func read(_ url: URL) async throws -> Peek {
        let bytes = UInt64((try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        if isZipling(url) {
            let listing = try await Engine.list(url)
            return Peek(format: "Zipling", entries: listing.entries.map {
                Entry(path: $0.path, isFolder: $0.isDirectory, size: $0.size,
                      modified: Date(timeIntervalSince1970: TimeInterval($0.mtime)), encrypted: false)
            }, archiveBytes: listing.archiveBytes, totalSize: listing.totalSize)
        }
        var entries: [Entry] = []
        for record in try ZipDirectory.records(url) {
            let name = record.name
            let path = name.split(separator: "/").filter { $0 != "." && $0 != ".." }.joined(separator: "/")
            guard !path.isEmpty else { continue }
            let isFolder = name.hasSuffix("/")
            entries.append(Entry(path: path, isFolder: isFolder, size: isFolder ? 0 : record.size,
                                 modified: record.modified, encrypted: record.encrypted))
        }
        // Folders that only appear in their files' paths.
        let listed = Set(entries.map(\.path))
        var implied = Set<String>()
        for entry in entries {
            var parent = (entry.path as NSString).deletingLastPathComponent
            while !parent.isEmpty, !listed.contains(parent), implied.insert(parent).inserted {
                parent = (parent as NSString).deletingLastPathComponent
            }
        }
        entries += implied.map { Entry(path: $0, isFolder: true, size: 0, modified: nil, encrypted: false) }
        return Peek(format: "ZIP", entries: entries, archiveBytes: bytes, totalSize: entries.reduce(0) { $0 + $1.size })
    }

    private static func isZipling(_ url: URL) -> Bool {
        guard let handle = try? FileHandle(forReadingFrom: url) else { return false }
        defer { try? handle.close() }
        return (try? handle.read(upToCount: 8)) == Data("ARCA\u{0}\r\n\u{1a}".utf8)
    }
}

/// The preview itself: a page in the system's look, light or dark.
enum PreviewPage {
    static let shownEntries = 3_000

    static func html(name: String, peek: Peek) -> String {
        let visible = peek.entries
            .filter { !$0.path.hasPrefix("__MACOSX/") && !($0.path as NSString).lastPathComponent.hasPrefix("._") }
            .sorted { $0.path.localizedStandardCompare($1.path) == .orderedAscending }
        let files = visible.filter { !$0.isFolder }
        let saved = peek.totalSize == 0 ? 0 : max(0, 1 - Double(peek.archiveBytes) / Double(peek.totalSize))
        let bytes = { (n: UInt64) in ByteCountFormatter.string(fromByteCount: Int64(clamping: n), countStyle: .file) }
        let date = Date.FormatStyle(date: .abbreviated, time: .shortened)
        var rows = ""
        for entry in visible.prefix(shownEntries) {
            let parts = entry.path.split(separator: "/")
            let depth = max(0, parts.count - 1)
            let leaf = escape(String(parts.last ?? ""))
            rows += """
            <tr><td style="padding-left:\(12 + depth * 16)px"><span class="icon">\(entry.isFolder ? "📁" : "📄")</span>\(leaf)\(entry.encrypted ? " <span class=\"lock\">🔒</span>" : "")</td>\
            <td class="num">\(entry.isFolder ? "" : bytes(entry.size))</td>\
            <td class="date">\(entry.modified.map { $0.formatted(date) } ?? "")</td></tr>
            """
        }
        let more = visible.count > shownEntries
            ? "<p class=\"more\">and \((visible.count - shownEntries).formatted()) more. Open it in Zipling to see everything.</p>" : ""
        return """
        <!doctype html><html><head><meta charset="utf-8"><style>
        :root { color-scheme: light dark; --dim: #6e6e73; --line: rgba(0,0,0,.08); --tint: #5856d6; }
        @media (prefers-color-scheme: dark) { :root { --dim: #98989d; --line: rgba(255,255,255,.1); --tint: #7d7aff; } }
        body { font: 13px -apple-system, system-ui; margin: 0; padding: 20px 22px; }
        h1 { font-size: 17px; margin: 0 0 4px; font-weight: 600; word-break: break-all; }
        .sub { color: var(--dim); margin-bottom: 14px; }
        .saved { color: var(--tint); font-weight: 600; }
        table { width: 100%; border-collapse: collapse; }
        th { text-align: left; font-weight: 500; color: var(--dim); font-size: 11px; padding: 4px 12px; border-bottom: 1px solid var(--line); }
        td { padding: 3px 12px; border-bottom: 1px solid var(--line); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 420px; }
        .icon { margin-right: 6px; }
        .num, .date { color: var(--dim); font-variant-numeric: tabular-nums; }
        .num { text-align: right; }
        .more { color: var(--dim); }
        </style></head><body>
        <h1>\(escape(name))</h1>
        <div class="sub">\(peek.format) · \(files.count.formatted()) file\(files.count == 1 ? "" : "s") · \(bytes(peek.totalSize)) → \(bytes(peek.archiveBytes)) \
        <span class="saved">\(saved.formatted(.percent.precision(.fractionLength(0)))) smaller</span></div>
        <table><thead><tr><th>Name</th><th style="text-align:right">Size</th><th>Modified</th></tr></thead>
        <tbody>\(rows)</tbody></table>\(more)
        </body></html>
        """
    }

    private static func escape(_ text: String) -> String {
        text.replacingOccurrences(of: "&", with: "&amp;").replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;").replacingOccurrences(of: "\"", with: "&quot;")
    }
}
