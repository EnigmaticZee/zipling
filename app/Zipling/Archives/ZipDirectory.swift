import Foundation

/// Reads a ZIP's central directory: every entry's raw name, flags, size and date,
/// without decompressing anything. The app uses it to spot names in a legacy code
/// page (7-Zip drops their bytes: a DOS "Größe.txt" lists as "Gre.txt"); the Quick
/// Look extension, which runs sandboxed, uses it to list ZIPs without a helper.
enum ZipDirectory {
    struct Record {
        let rawName: Data
        let flags: UInt16
        let size: UInt64
        let modified: Date?
        /// Info-ZIP's Unicode Path extra field (0x7075) carries a UTF-8 copy of the name.
        let hasUnicodePath: Bool

        var encrypted: Bool { flags & 1 != 0 }

        /// UTF-8 when flagged or valid, otherwise ZIP's default, the DOS code page.
        var name: String {
            if flags & 0x800 != 0 { return String(decoding: rawName, as: UTF8.self) }
            return String(data: rawName, encoding: .utf8) ?? String(data: rawName, encoding: ZipDirectory.dos)
                ?? String(decoding: rawName, as: UTF8.self)
        }

        /// Non-ASCII bytes, no UTF-8 flag or Unicode path, and not valid UTF-8 anyway
        /// (as Mac and Linux tools write): a legacy code page such as Shift-JIS or CP437.
        var needsDecoding: Bool {
            flags & 0x800 == 0 && !hasUnicodePath && rawName.contains { $0 >= 0x80 }
                && String(data: rawName, encoding: .utf8) == nil
        }
    }

    /// True when some name is in a legacy code page.
    static func needsDecoding(_ url: URL) -> Bool {
        (try? records(url))?.contains(where: \.needsDecoding) ?? false
    }

    static func records(_ url: URL) throws -> [Record] {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let size = try handle.seekToEnd()
        guard size >= 22 else { throw failure }
        // The end record sits in the last 22 bytes plus up to 64 KB of comment.
        let tailLength = min(size, 22 + 65_535)
        try handle.seek(toOffset: size - tailLength)
        guard let tail = try handle.read(upToCount: Int(tailLength)), let end = lastIndex(of: 0x0605_4b50, in: tail)
        else { throw failure }
        var directorySize = UInt64(tail.u32(end + 12)), directoryOffset = UInt64(tail.u32(end + 16))
        if directorySize == 0xffff_ffff || directoryOffset == 0xffff_ffff {
            // ZIP64: a locator just before the end record points at the real one.
            guard end >= 20, tail.u32(end - 20) == 0x0706_4b50 else { throw failure }
            try handle.seek(toOffset: tail.u64(end - 12))
            guard let record = try handle.read(upToCount: 56), record.count == 56, record.u32(0) == 0x0606_4b50
            else { throw failure }
            directorySize = record.u64(40)
            directoryOffset = record.u64(48)
        }
        guard directorySize <= 256 << 20 else { throw failure }
        func directory(at offset: UInt64) throws -> Data? {
            try handle.seek(toOffset: offset)
            guard let data = try handle.read(upToCount: Int(directorySize)), data.count >= 4,
                  data.u32(0) == 0x0201_4b50 else { return nil }
            return data
        }
        // Self-extracting and prefixed ZIPs count offsets from where the ZIP starts,
        // so the directory is found just before the end record instead.
        let endOffset = size - tailLength + UInt64(end)
        guard let data = try directory(at: directoryOffset)
                ?? (endOffset >= directorySize ? directory(at: endOffset - directorySize) : nil) else { throw failure }

        var records: [Record] = []
        var i = 0
        while i + 46 <= data.count, data.u32(i) == 0x0201_4b50 {
            let flags = data.u16(i + 8)
            let time = data.u16(i + 12), day = data.u16(i + 14)
            var size = UInt64(data.u32(i + 24))
            let nameLength = Int(data.u16(i + 28)), extraLength = Int(data.u16(i + 30)), commentLength = Int(data.u16(i + 32))
            let start = i + 46
            guard start + nameLength + extraLength <= data.count else { break }
            let name = data.subdata(in: data.startIndex + start ..< data.startIndex + start + nameLength)
            let extra = extraFields(data, from: start + nameLength, length: extraLength)
            if size == 0xffff_ffff, let big = extra[0x0001], big.count >= 8 { size = big.u64(0) }
            records.append(Record(rawName: name, flags: flags, size: size, modified: dosDate(day: day, time: time),
                                  hasUnicodePath: extra[0x7075] != nil))
            i = start + nameLength + extraLength + commentLength
        }
        return records
    }

    private static let failure = CocoaError(.fileReadCorruptFile)
    static let dos = String.Encoding(rawValue: CFStringConvertEncodingToNSStringEncoding(
        CFStringEncoding(CFStringEncodings.dosLatinUS.rawValue)))

    private static func extraFields(_ data: Data, from start: Int, length: Int) -> [UInt16: Data] {
        var fields: [UInt16: Data] = [:]
        var i = start
        while i + 4 <= start + length {
            let id = data.u16(i), count = Int(data.u16(i + 2))
            guard i + 4 + count <= start + length else { break }
            fields[id] = data.subdata(in: data.startIndex + i + 4 ..< data.startIndex + i + 4 + count)
            i += 4 + count
        }
        return fields
    }

    private static func dosDate(day: UInt16, time: UInt16) -> Date? {
        guard day != 0 else { return nil }
        var parts = DateComponents()
        parts.year = 1980 + Int(day >> 9)
        parts.month = Int((day >> 5) & 0xf)
        parts.day = Int(day & 0x1f)
        parts.hour = Int(time >> 11)
        parts.minute = Int((time >> 5) & 0x3f)
        parts.second = Int(time & 0x1f) * 2
        return Calendar.current.date(from: parts)
    }

    private static func lastIndex(of signature: UInt32, in data: Data) -> Int? {
        var i = data.count - 22
        while i >= 0 {
            if data.u32(i) == signature { return i }
            i -= 1
        }
        return nil
    }
}

private extension Data {
    func u16(_ at: Int) -> UInt16 {
        let i = startIndex + at
        return UInt16(self[i]) | UInt16(self[i + 1]) << 8
    }

    func u32(_ at: Int) -> UInt32 { UInt32(u16(at)) | UInt32(u16(at + 2)) << 16 }

    func u64(_ at: Int) -> UInt64 { UInt64(u32(at)) | UInt64(u32(at + 4)) << 32 }
}
