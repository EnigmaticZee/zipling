import AppKit
import UniformTypeIdentifiers

extension UTType {
    static let ziplingArchive = UTType(exportedAs: "com.observantsystems.zipling-archive")
}

enum Format {
    static func bytes(_ n: UInt64) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(clamping: n), countStyle: .file)
    }

    static func percent(_ fraction: Double) -> String {
        fraction.formatted(.percent.precision(.fractionLength(0)))
    }

    static func duration(_ seconds: Double) -> String {
        if seconds < 1 { return "\(Int((seconds * 1000).rounded())) ms" }
        if seconds < 60 { return seconds.formatted(.number.precision(.fractionLength(1))) + " s" }
        return Duration.seconds(seconds).formatted(.units(allowed: [.hours, .minutes, .seconds], width: .abbreviated))
    }

    static func rate(bytes: UInt64, seconds: Double) -> String {
        guard seconds > 0 else { return "" }
        return Self.bytes(UInt64(Double(bytes) / seconds)) + "/s"
    }

    static func count(_ n: UInt64, _ noun: String) -> String {
        "\(n.formatted()) \(noun)\(n == 1 ? "" : "s")"
    }
}

/// Plain-language names for how a file was stored.
enum Method {
    static func label(transform: String, codec: String) -> String {
        switch transform {
        case "lepton": "Lepton"
        case "deflate": "Unpacked + \(shortCodec(codec))"
        case "arm64": "ARM64 + \(shortCodec(codec))"
        case "foreign": foreignName(codec)
        default: codecName(codec)
        }
    }

    /// Methods named by other formats, as 7-Zip reports them ("LZMA2:24",
    /// "AES-256 Deflate", "LZMA:24 BCJ"): parameters dropped, steps joined.
    static func foreignName(_ method: String) -> String {
        let steps = method.split(separator: " ").map { $0.split(separator: ":").first.map(String.init) ?? String($0) }
        guard !steps.isEmpty else { return "—" }
        // 7-Zip says Copy and The Unarchiver says None for files kept as they are.
        return steps.map { $0 == "Copy" || $0 == "None" ? "Stored" : $0 }.joined(separator: " + ")
    }

    static func shortCodec(_ codec: String) -> String {
        codec == "zstd" ? "zstd" : codecName(codec)
    }

    static func codecName(_ codec: String) -> String {
        switch codec {
        case "zstd": "Zstandard"
        case "xz": "LZMA2"
        case "ppmd": "PPMd"
        default: "Stored"
        }
    }

    static func explanation(transform: String) -> String {
        switch transform {
        case "lepton": "This JPEG was re-encoded losslessly with Lepton. It comes back bit for bit identical, about a fifth smaller."
        case "deflate": "This file's internal Deflate compression (ZIP, Office, PNG or PDF) was undone, so a much stronger codec could see the real data. It is rebuilt exactly on extraction."
        case "arm64": "The Apple silicon code in this app binary was rearranged so repeated calls look alike and compress better. Reversed exactly on extraction."
        case "foreign": "Stored the way the app that made this archive chose."
        default: "Compressed as-is."
        }
    }

    static func symbol(transform: String) -> String {
        switch transform {
        case "lepton": "photo"
        case "deflate": "doc.zipper"
        case "arm64": "cpu"
        default: "square.stack.3d.down.right"
        }
    }
}

/// Finder icons by file type, cached.
@MainActor
enum FileIcons {
    private static var cache: [String: NSImage] = [:]

    static func icon(forName name: String, isFolder: Bool) -> NSImage {
        let key = isFolder ? "/" : (name as NSString).pathExtension.lowercased()
        if let hit = cache[key] { return hit }
        let type: UTType = isFolder ? .folder : (UTType(filenameExtension: key) ?? .data)
        let image = NSWorkspace.shared.icon(for: type)
        cache[key] = image
        return image
    }
}

enum Placement {
    /// `folder/name`, or `folder/name 2`, `name 3`... if taken (the way Finder does it).
    static func unique(_ name: String, in folder: URL) -> URL {
        var candidate = folder.appending(path: name)
        guard exists(candidate) else { return candidate }
        let base = (name as NSString).deletingPathExtension
        let ext = (name as NSString).pathExtension
        for n in 2... {
            let next = ext.isEmpty ? "\(base) \(n)" : "\(base) \(n).\(ext)"
            candidate = folder.appending(path: next)
            if !exists(candidate) { return candidate }
        }
        return candidate
    }

    /// Whether anything is at `url`, a symbolic link included (even one whose target is gone).
    static func exists(_ url: URL) -> Bool {
        var info = stat()
        return lstat(url.path, &info) == 0
    }
}
