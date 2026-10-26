import Darwin
import Foundation

/// The last step of every extraction, run on the hidden staging folder before
/// anything is moved into place:
///
/// 1. Mac metadata stored as AppleDouble (`__MACOSX/…/._name` or `._name`) is merged
///    back into the real files: Finder tags, comments, custom icons, resource forks.
///    Only extended attributes are restored, never ACLs (Microsoft's 2022 "Achilles"
///    Gatekeeper bypass used an AppleDouble ACL), and any quarantine they carry is
///    dropped in favour of the archive's own.
/// 2. Symbolic links that are absolute, or resolve outside the extracted items, are
///    removed.
/// 3. The archive's quarantine ("downloaded from the internet") is copied onto every
///    extracted item, as Archive Utility does, so Gatekeeper still checks apps and
///    documents that came out of a downloaded archive.
enum SafeExtraction {
    struct Report {
        /// Paths (relative to the staging folder) of links that pointed outside it.
        var removedLinks: [String] = []
    }

    static let quarantineName = "com.apple.quarantine"

    /// The archive's quarantine attribute, if it was downloaded.
    static func quarantine(of url: URL) -> Data? {
        xattr(quarantineName, of: url.path)
    }

    static func finalize(_ staging: URL, quarantine: Data?) throws -> Report {
        let root = staging.standardizedFileURL.path
        let realRoot = realPath(root) ?? root
        var report = Report()
        mergeAppleDouble(in: root)

        for path in allItems(under: root) {
            var info = stat()
            guard lstat(path, &info) == 0 else { continue }
            if info.st_mode & S_IFMT == S_IFLNK, !linkStaysInside(path, root: root, realRoot: realRoot) {
                unlink(path)
                report.removedLinks.append(String(path.dropFirst(root.count + 1)))
                continue
            }
            if let quarantine {
                _ = quarantine.withUnsafeBytes {
                    setxattr(path, quarantineName, $0.baseAddress, quarantine.count, 0, XATTR_NOFOLLOW)
                }
            }
        }
        return report
    }

    /// Applies each AppleDouble file to the item it describes, then deletes it.
    private static func mergeAppleDouble(in root: String) {
        let items = allItems(under: root)
        let sequestered = root + "/__MACOSX"
        for path in items {
            let name = (path as NSString).lastPathComponent
            guard name.hasPrefix("._") else { continue }
            let dir = (path as NSString).deletingLastPathComponent
            // __MACOSX/a/b/._x describes a/b/x; a loose ._x describes the x beside it.
            let targetDir = dir.hasPrefix(sequestered) ? root + String(dir.dropFirst(sequestered.count)) : dir
            let target = targetDir + "/" + String(name.dropFirst(2))
            var info = stat()
            if lstat(target, &info) == 0 {
                let flags = copyfile_flags_t(COPYFILE_UNPACK | COPYFILE_XATTR | COPYFILE_NOFOLLOW)
                if copyfile(path, target, nil, flags) == 0 {
                    removexattr(target, quarantineName, XATTR_NOFOLLOW)
                }
            }
            unlink(path)
        }
        try? FileManager.default.removeItem(atPath: sequestered)
    }

    /// True when the link resolves inside the extracted items. Links whose target
    /// exists are resolved for real; dangling ones are judged by their text.
    private static func linkStaysInside(_ link: String, root: String, realRoot: String) -> Bool {
        guard let target = try? FileManager.default.destinationOfSymbolicLink(atPath: link) else { return false }
        if target.hasPrefix("/") { return false }
        if let real = realPath(link) {
            return real == realRoot || real.hasPrefix(realRoot + "/")
        }
        let parent = (link as NSString).deletingLastPathComponent
        let lexical = URL(filePath: parent + "/" + target).standardizedFileURL.path
        return lexical == root || lexical.hasPrefix(root + "/")
    }

    /// Every item below `root`, depth first, without following symbolic links.
    /// (Foundation's directory enumerators silently skip `._` AppleDouble files, which
    /// is exactly what this needs to see.)
    static func allItems(under root: String) -> [String] {
        var out: [String] = []
        var folders = [root]
        while let folder = folders.popLast() {
            for name in names(in: folder) {
                let path = folder + "/" + name
                out.append(path)
                var info = stat()
                if lstat(path, &info) == 0, info.st_mode & S_IFMT == S_IFDIR { folders.append(path) }
            }
        }
        return out
    }

    /// The names directly inside a folder, hidden and `._` ones included.
    static func names(in folder: String) -> [String] {
        guard let dir = opendir(folder) else { return [] }
        defer { closedir(dir) }
        var out: [String] = []
        while let entry = readdir(dir) {
            let name = withUnsafeBytes(of: entry.pointee.d_name) { raw in
                String(decoding: raw.prefix(Int(entry.pointee.d_namlen)), as: UTF8.self)
            }
            if name != "." && name != ".." { out.append(name) }
        }
        return out
    }

    private static func realPath(_ path: String) -> String? {
        guard let resolved = realpath(path, nil) else { return nil }
        defer { free(resolved) }
        return String(cString: resolved)
    }

    static func xattr(_ name: String, of path: String) -> Data? {
        let size = getxattr(path, name, nil, 0, 0, XATTR_NOFOLLOW)
        guard size > 0 else { return nil }
        var data = Data(count: size)
        let read = data.withUnsafeMutableBytes { getxattr(path, name, $0.baseAddress, size, 0, XATTR_NOFOLLOW) }
        return read == size ? data : nil
    }
}
