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

    /// `alreadyQuarantined`: the engine set the quarantine while writing, so it is
    /// only set again here if merging AppleDouble files took it off some items.
    static func finalize(_ staging: URL, quarantine: Data?, alreadyQuarantined: Bool = false) throws -> Report {
        let root = staging.standardizedFileURL.path
        let realRoot = realPath(root) ?? root
        var report = Report()
        // One walk for every step: the directory listing already says what each item is.
        var items = allItems(under: root)
        let merged = mergeAppleDouble(items, in: root)
        if merged {
            let sequestered = root + "/__MACOSX"
            items.removeAll { item in
                (item.path as NSString).lastPathComponent.hasPrefix("._")
                    || item.path == sequestered || item.path.hasPrefix(sequestered + "/")
            }
        }

        var unsafe = Set<String>()
        for item in items where item.isLink && !linkStaysInside(item.path, root: root, realRoot: realRoot) {
            unlink(item.path)
            unsafe.insert(item.path)
            report.removedLinks.append(String(item.path.dropFirst(root.count + 1)))
        }
        if let quarantine, !alreadyQuarantined || merged {
            let marked = unsafe.isEmpty ? items : items.filter { !unsafe.contains($0.path) }
            quarantine.withUnsafeBytes { value in
                DispatchQueue.concurrentPerform(iterations: marked.count) { i in
                    setxattr(marked[i].path, quarantineName, value.baseAddress, value.count, 0, XATTR_NOFOLLOW)
                }
            }
        }
        return report
    }

    /// Applies each AppleDouble file to the item it describes, then deletes it.
    /// Returns whether there were any.
    private static func mergeAppleDouble(_ items: [Item], in root: String) -> Bool {
        let sequestered = root + "/__MACOSX"
        var found = false
        for item in items {
            let path = item.path
            let name = (path as NSString).lastPathComponent
            guard name.hasPrefix("._") else { continue }
            found = true
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
        if items.contains(where: { $0.path == sequestered }) {
            try? FileManager.default.removeItem(atPath: sequestered)
            found = true
        }
        return found
    }

    /// Applies Mac metadata read straight from an archive: `data` holds each entry's
    /// AppleDouble file back to back, in the order of `entries`. Returns false, having
    /// applied nothing, when the sizes don't add up.
    static func applyAppleDouble(_ data: Data, entries: [MacMetadata], under root: String) -> Bool {
        guard entries.reduce(0, { $0 + $1.size }) == UInt64(data.count) else { return false }
        let realRoot = realPath(root) ?? root
        var jobs: [(target: String, bytes: Range<Int>)] = []
        var offset = 0
        for entry in entries {
            let bytes = offset..<(offset + Int(entry.size))
            offset = bytes.upperBound
            // __MACOSX/a/b/._x describes a/b/x; a loose ._x describes the x beside it.
            var parts = SevenZip.cleanPath(entry.path).split(separator: "/").map(String.init)
            if parts.first == "__MACOSX" { parts.removeFirst() }
            guard let name = parts.last, name.hasPrefix("._"), name.count > 2 else { continue }
            parts[parts.count - 1] = String(name.dropFirst(2))
            jobs.append((root + "/" + parts.joined(separator: "/"), bytes))
        }
        data.withUnsafeBytes { raw in
            let all = raw.bindMemory(to: UInt8.self)
            DispatchQueue.concurrentPerform(iterations: jobs.count) { i in
                let (target, bytes) = jobs[i]
                // Never through a folder link that leads out of the extracted items.
                let parent = (target as NSString).deletingLastPathComponent
                guard let real = realPath(parent), real == realRoot || real.hasPrefix(realRoot + "/") else { return }
                apply(UnsafeBufferPointer(rebasing: all[bytes]), to: target)
            }
        }
        return true
    }

    /// One AppleDouble file's extended attributes, Finder info and resource fork, set
    /// on `target` as `copyfile` would, without quarantine or ACLs. Anything that isn't
    /// the layout macOS writes goes through `copyfile` itself.
    private static func apply(_ appleDouble: UnsafeBufferPointer<UInt8>, to target: String) {
        var info = stat()
        guard lstat(target, &info) == 0 else { return }
        guard let attributes = AppleDouble.attributes(appleDouble) else {
            unpack(appleDouble, to: target)
            return
        }
        for (name, bytes) in attributes where name != quarantineName && name != AppleDouble.aclName {
            let value = UnsafeBufferPointer(rebasing: appleDouble[bytes])
            setxattr(target, name, value.baseAddress, value.count, 0, XATTR_NOFOLLOW)
            // Like copyfile, an item Finder hides is hidden for everything else too.
            if name == AppleDouble.finderInfoName, value.count >= 10, value[8] & 0x40 != 0 {
                lchflags(target, info.st_flags | UInt32(UF_HIDDEN))
            }
        }
    }

    /// `copyfile`'s own AppleDouble unpacking, through a temporary file.
    private static func unpack(_ appleDouble: UnsafeBufferPointer<UInt8>, to target: String) {
        let file = FileManager.default.temporaryDirectory.appending(path: "zipling-ad-\(UUID().uuidString)").path
        guard FileManager.default.createFile(atPath: file, contents: Data(buffer: appleDouble)) else { return }
        defer { unlink(file) }
        let flags = copyfile_flags_t(COPYFILE_UNPACK | COPYFILE_XATTR | COPYFILE_NOFOLLOW)
        if copyfile(file, target, nil, flags) == 0 {
            removexattr(target, quarantineName, XATTR_NOFOLLOW)
        }
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

    struct Item {
        let path: String
        let isLink: Bool
    }

    /// Every item below `root`, depth first, without following symbolic links.
    /// (Foundation's directory enumerators silently skip `._` AppleDouble files, which
    /// is exactly what this needs to see.)
    static func allItems(under root: String) -> [Item] {
        var out: [Item] = []
        var folders = [root]
        while let folder = folders.popLast() {
            for (name, type) in entries(in: folder) {
                let path = folder + "/" + name
                var kind = Int32(type)
                if kind == DT_UNKNOWN {  // a file system that doesn't say: ask
                    var info = stat()
                    kind = lstat(path, &info) != 0 ? DT_UNKNOWN
                        : info.st_mode & S_IFMT == S_IFDIR ? DT_DIR : info.st_mode & S_IFMT == S_IFLNK ? DT_LNK : DT_REG
                }
                out.append(Item(path: path, isLink: kind == DT_LNK))
                if kind == DT_DIR { folders.append(path) }
            }
        }
        return out
    }

    /// The names directly inside a folder, hidden and `._` ones included.
    static func names(in folder: String) -> [String] {
        entries(in: folder).map(\.name)
    }

    /// Each name in a folder with its type as the directory itself records it.
    private static func entries(in folder: String) -> [(name: String, type: UInt8)] {
        guard let dir = opendir(folder) else { return [] }
        defer { closedir(dir) }
        var out: [(name: String, type: UInt8)] = []
        while let entry = readdir(dir) {
            let name = withUnsafeBytes(of: entry.pointee.d_name) { raw in
                String(decoding: raw.prefix(Int(entry.pointee.d_namlen)), as: UTF8.self)
            }
            if name != "." && name != ".." { out.append((name, entry.pointee.d_type)) }
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

/// The AppleDouble layout macOS writes (`copyfile` and `ditto`) for metadata a file
/// system or archive can't hold: a big-endian header, Finder info with the extended
/// attributes appended after an "ATTR" header, then the resource fork.
enum AppleDouble {
    static let finderInfoName = "com.apple.FinderInfo"
    static let resourceForkName = "com.apple.ResourceFork"
    /// How `copyfile` packs an ACL. Zipling never restores those.
    static let aclName = "com.apple.acl.text"

    /// Every attribute to set, with where its value lies. The Finder info and the
    /// resource fork come out as the attributes macOS shows them as. Nil for anything
    /// but version 2 laid out the way macOS writes it.
    static func attributes(_ b: UnsafeBufferPointer<UInt8>) -> [(name: String, bytes: Range<Int>)]? {
        func u16(_ i: Int) -> Int? { i >= 0 && i + 2 <= b.count ? Int(b[i]) << 8 | Int(b[i + 1]) : nil }
        func u32(_ i: Int) -> Int? {
            i >= 0 && i + 4 <= b.count ? Int(b[i]) << 24 | Int(b[i + 1]) << 16 | Int(b[i + 2]) << 8 | Int(b[i + 3]) : nil
        }
        guard u32(0) == 0x0005_1607, u32(4) == 0x0002_0000, let count = u16(24) else { return nil }
        var out: [(name: String, bytes: Range<Int>)] = []
        for entry in 0..<count {
            let at = 26 + entry * 12
            guard let id = u32(at), let offset = u32(at + 4), let length = u32(at + 8), offset + length <= b.count
            else { return nil }
            if id == 2, length > 0 {
                out.append((resourceForkName, offset..<(offset + length)))
            } else if id == 9 {
                guard length >= 32 else { return nil }
                if b[offset..<(offset + 32)].contains(where: { $0 != 0 }) {
                    out.append((finderInfoName, offset..<(offset + 32)))
                }
                guard length > 32 else { continue }
                // Two bytes of padding, then the attribute header: "ATTR", four words,
                // three reserved words, flags and the attribute count.
                let header = offset + 34
                guard u32(header) == 0x4154_5452, let attrs = u16(header + 34) else { return nil }
                var at = header + 36
                for _ in 0..<attrs {
                    guard let start = u32(at), let size = u32(at + 4), at + 11 <= b.count else { return nil }
                    let nameLength = Int(b[at + 10])  // with its terminating NUL
                    guard nameLength > 1, at + 11 + nameLength <= b.count, start + size <= b.count else { return nil }
                    let name = String(decoding: UnsafeBufferPointer(rebasing: b[(at + 11)..<(at + 10 + nameLength)]),
                                      as: UTF8.self)
                    out.append((name, start..<(start + size)))
                    at = (at + 11 + nameLength + 3) & ~3
                }
            }
        }
        return out
    }
}
