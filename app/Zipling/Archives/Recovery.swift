import CryptoKit
import Foundation

/// Recovery files: repair data saved next to an archive ("Photos.zip.recovery") that
/// can rebuild damaged parts of it, as PAR2 does. They work for any archive and
/// change nothing in it.
///
/// The archive is cut into k equal slices; the recovery file holds m parity slices,
/// Reed–Solomon codes over them (a Cauchy matrix over GF(256)), and a SHA-256 of
/// every slice. Any m damaged slices, wherever they are, can be found by their
/// hashes and rebuilt from the rest.
enum Recovery {
    static let fileExtension = "recovery"
    private static let magic = Data("ZPRECOV1".utf8)
    /// Bytes worked on at a time from each slice.
    private static let stripe = 256 << 10

    struct Header: Equatable {
        let archiveSize: UInt64
        let sliceSize: Int
        let dataHashes: [Data]
        let parityHashes: [Data]
        var k: Int { dataHashes.count }
        var m: Int { parityHashes.count }
    }

    /// What a check found.
    struct Report: Sendable, Equatable {
        /// Damaged (or missing) slices of the archive, and how many can be rebuilt.
        let damagedSlices: Int
        let repairableSlices: Int
        let sliceSize: Int
        var intact: Bool { damagedSlices == 0 }
        var repairable: Bool { damagedSlices <= repairableSlices }
    }

    static func file(for archive: URL) -> URL { archive.appendingPathExtension(fileExtension) }

    /// How much damage a recovery file can repair, in bytes.
    static func capacity(of recovery: URL) -> UInt64? {
        guard let header = try? readHeader(recovery) else { return nil }
        return UInt64(header.m * header.sliceSize)
    }

    /// Makes the recovery file again after the archive changed, with the same strength.
    static func refresh(for archive: URL, cancel: CancelToken) async throws {
        let recovery = file(for: archive)
        guard let header = try? readHeader(recovery) else { return }
        try await create(for: archive, redundancy: Double(header.m) / Double(header.k), cancel: cancel) { _, _ in }
    }

    // MARK: Making

    /// Writes a recovery file for `archive`. `redundancy` is the share of the archive's
    /// size that can be rebuilt: 0.1 repairs up to a tenth of it.
    @discardableResult
    static func create(
        for archive: URL, redundancy: Double, cancel: CancelToken,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws -> URL {
        let output = file(for: archive)
        try await Task.detached(priority: .utility) {
            try write(archive, to: output, redundancy: redundancy, cancel: cancel, progress: progress)
        }.value
        return output
    }

    private static func layout(size: UInt64, redundancy: Double) -> (k: Int, m: Int, slice: Int) {
        let r = min(max(redundancy, 0.01), 1)
        let kMax = Int((255 / (1 + r)).rounded(.down))
        var slice = Int((Double(size) / Double(kMax)).rounded(.up))
        slice = max(64, (slice + 63) / 64 * 64)
        let k = max(1, Int((Double(size) / Double(slice)).rounded(.up)))
        let m = max(1, min(255 - k, Int((Double(k) * r).rounded(.up))))
        return (k, m, slice)
    }

    private static func write(
        _ archive: URL, to output: URL, redundancy: Double, cancel: CancelToken,
        progress: @Sendable (UInt64, UInt64) -> Void
    ) throws {
        let size = (try FileManager.default.attributesOfItem(atPath: archive.path)[.size] as? NSNumber)?.uint64Value ?? 0
        let (k, m, slice) = layout(size: size, redundancy: redundancy)
        let matrix = cauchy(k: k, m: m)
        let input = try FileHandle(forReadingFrom: archive)
        defer { try? input.close() }
        let partial = output.appendingPathExtension("partial")
        FileManager.default.createFile(atPath: partial.path, contents: nil)
        let out = try FileHandle(forWritingTo: partial)
        var finished = false
        defer {
            try? out.close()
            if !finished { try? FileManager.default.removeItem(at: partial) }
        }
        let headerLength = headerSize(k: k, m: m)
        var dataHashers = Array(repeating: SHA256(), count: k)
        var parityHashers = Array(repeating: SHA256(), count: m)

        var offset = 0
        while offset < slice {
            if cancel.isCancelled { throw EngineError(message: "cancelled") }
            let length = min(stripe, slice - offset)
            let data = (0..<k).map { readSlice(input, index: $0, slice: slice, offset: offset, length: length) }
            for i in 0..<k { dataHashers[i].update(data: data[i]) }
            let parity = encode(data, matrix: matrix, length: length)
            for j in 0..<m {
                parityHashers[j].update(data: parity[j])
                try out.seek(toOffset: UInt64(headerLength + j * slice + offset))
                try out.write(contentsOf: parity[j])
            }
            offset += length
            progress(UInt64(offset), UInt64(slice))
        }
        let header = Header(archiveSize: size, sliceSize: slice,
                            dataHashes: dataHashers.map { Data($0.finalize()) },
                            parityHashes: parityHashers.map { Data($0.finalize()) })
        try out.seek(toOffset: 0)
        try out.write(contentsOf: encodeHeader(header))
        try out.close()
        _ = try? FileManager.default.removeItem(at: output)
        try FileManager.default.moveItem(at: partial, to: output)
        finished = true
    }

    // MARK: Checking and repairing

    /// Finds which slices of the archive are damaged.
    static func check(_ archive: URL, recovery: URL, cancel: CancelToken) async throws -> Report {
        try await Task.detached(priority: .userInitiated) {
            let header = try readHeader(recovery)
            let damaged = try damagedSlices(archive, header: header, cancel: cancel)
            let goodParity = try goodParitySlices(recovery, header: header, cancel: cancel)
            return Report(damagedSlices: damaged.count, repairableSlices: goodParity.count, sliceSize: header.sliceSize)
        }.value
    }

    /// Writes a repaired copy of the archive to `output`, checked slice by slice.
    static func repair(
        _ archive: URL, recovery: URL, to output: URL, cancel: CancelToken,
        progress: @escaping @Sendable (UInt64, UInt64) -> Void
    ) async throws {
        try await Task.detached(priority: .userInitiated) {
            try rebuild(archive, recovery: recovery, to: output, cancel: cancel, progress: progress)
        }.value
    }

    private static func rebuild(
        _ archive: URL, recovery: URL, to output: URL, cancel: CancelToken,
        progress: @Sendable (UInt64, UInt64) -> Void
    ) throws {
        let header = try readHeader(recovery)
        let (k, slice) = (header.k, header.sliceSize)
        let erased = try damagedSlices(archive, header: header, cancel: cancel)
        let parityRows = try goodParitySlices(recovery, header: header, cancel: cancel)
        guard erased.count <= parityRows.count else {
            throw EngineError(message: "Too much of the archive is damaged to repair: \(erased.count) of \(k) parts, and the recovery file can rebuild \(parityRows.count).")
        }
        let fm = FileManager.default
        let partial = output.appendingPathExtension("partial")
        try? fm.removeItem(at: partial)
        try fm.copyItem(at: archive, to: partial)
        var finished = false
        defer { if !finished { try? fm.removeItem(at: partial) } }
        let out = try FileHandle(forUpdating: partial)
        defer { try? out.close() }
        try out.truncate(atOffset: header.archiveSize)

        if !erased.isEmpty {
            let rows = Array(parityRows.prefix(erased.count))
            let matrix = cauchy(k: k, m: header.m)
            // The rows of the code that cover the damaged slices, inverted.
            let inverse = try invert(rows.map { row in erased.map { matrix[row][$0] } })
            let input = try FileHandle(forReadingFrom: archive)
            defer { try? input.close() }
            let parityFile = try FileHandle(forReadingFrom: recovery)
            defer { try? parityFile.close() }
            let headerLength = headerSize(k: k, m: header.m)
            let erasedSet = Set(erased)
            var offset = 0
            while offset < slice {
                if cancel.isCancelled { throw EngineError(message: "cancelled") }
                let length = min(stripe, slice - offset)
                // What the damaged slices must add up to, from each parity row.
                var syndromes = try rows.map { row -> [UInt8] in
                    try parityFile.seek(toOffset: UInt64(headerLength + row * slice + offset))
                    var bytes = [UInt8](try parityFile.read(upToCount: length) ?? Data())
                    if bytes.count < length { bytes += [UInt8](repeating: 0, count: length - bytes.count) }
                    return bytes
                }
                for i in 0..<k where !erasedSet.contains(i) {
                    let data = [UInt8](readSlice(input, index: i, slice: slice, offset: offset, length: length))
                    for (r, row) in rows.enumerated() { GF.mulAdd(&syndromes[r], data, matrix[row][i]) }
                }
                for (c, index) in erased.enumerated() {
                    var rebuilt = [UInt8](repeating: 0, count: length)
                    for r in 0..<rows.count { GF.mulAdd(&rebuilt, syndromes[r], inverse[c][r]) }
                    let start = UInt64(index * slice + offset)
                    guard start < header.archiveSize else { continue }
                    let keep = Int(min(UInt64(length), header.archiveSize - start))
                    try out.seek(toOffset: start)
                    try out.write(contentsOf: Data(rebuilt.prefix(keep)))
                }
                offset += length
                progress(UInt64(offset), UInt64(slice))
            }
        }
        try out.synchronize()
        try out.close()
        // Every slice must now match what was recorded.
        guard try damagedSlices(partial, header: header, cancel: cancel).isEmpty else {
            throw EngineError(message: "The repair didn't check out, so nothing was written.")
        }
        try? fm.removeItem(at: output)
        try fm.moveItem(at: partial, to: output)
        finished = true
    }

    private static func damagedSlices(_ archive: URL, header: Header, cancel: CancelToken) throws -> [Int] {
        let input = try? FileHandle(forReadingFrom: archive)
        defer { try? input?.close() }
        var damaged: [Int] = []
        for i in 0..<header.k {
            if cancel.isCancelled { throw EngineError(message: "cancelled") }
            var hasher = SHA256()
            var offset = 0
            while offset < header.sliceSize {
                let length = min(stripe, header.sliceSize - offset)
                hasher.update(data: input.map { readSlice($0, index: i, slice: header.sliceSize, offset: offset, length: length) }
                              ?? Data(count: length))
                offset += length
            }
            if Data(hasher.finalize()) != header.dataHashes[i] { damaged.append(i) }
        }
        return damaged
    }

    private static func goodParitySlices(_ recovery: URL, header: Header, cancel: CancelToken) throws -> [Int] {
        let file = try FileHandle(forReadingFrom: recovery)
        defer { try? file.close() }
        let headerLength = headerSize(k: header.k, m: header.m)
        var good: [Int] = []
        for j in 0..<header.m {
            if cancel.isCancelled { throw EngineError(message: "cancelled") }
            try file.seek(toOffset: UInt64(headerLength + j * header.sliceSize))
            var hasher = SHA256()
            var left = header.sliceSize
            while left > 0 {
                let chunk = try file.read(upToCount: min(stripe, left)) ?? Data()
                if chunk.isEmpty { break }
                hasher.update(data: chunk)
                left -= chunk.count
            }
            if left == 0, Data(hasher.finalize()) == header.parityHashes[j] { good.append(j) }
        }
        return good
    }

    /// One stripe of slice `index`, zero-padded past the end of the file.
    private static func readSlice(_ handle: FileHandle, index: Int, slice: Int, offset: Int, length: Int) -> Data {
        let start = UInt64(index * slice + offset)
        var data = Data()
        if (try? handle.seek(toOffset: start)) != nil { data = (try? handle.read(upToCount: length)) ?? Data() }
        if data.count < length { data.append(Data(count: length - data.count)) }
        return data
    }

    /// Each parity row on its own core, each into its own part of one buffer.
    private static func encode(_ data: [Data], matrix: [[UInt8]], length: Int) -> [Data] {
        let rows = matrix.count
        let inputs = data.map { [UInt8]($0) }
        var parity = [UInt8](repeating: 0, count: rows * length)
        parity.withUnsafeMutableBufferPointer { buffer in
            nonisolated(unsafe) let base = buffer.baseAddress!
            DispatchQueue.concurrentPerform(iterations: rows) { j in
                let out = UnsafeMutableBufferPointer(start: base + j * length, count: length)
                for (i, input) in inputs.enumerated() {
                    input.withUnsafeBufferPointer { GF.mulAdd(out, $0, matrix[j][i]) }
                }
            }
        }
        return (0..<rows).map { Data(parity[$0 * length ..< ($0 + 1) * length]) }
    }

    // MARK: The file's header

    private static func headerSize(k: Int, m: Int) -> Int { magic.count + 8 + 8 + 2 + 2 + 32 * (k + m) + 32 }

    private static func encodeHeader(_ header: Header) -> Data {
        var data = magic
        func put<T: FixedWidthInteger>(_ value: T) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
        put(header.archiveSize)
        put(UInt64(header.sliceSize))
        put(UInt16(header.k))
        put(UInt16(header.m))
        for hash in header.dataHashes + header.parityHashes { data.append(hash) }
        data.append(Data(SHA256.hash(data: data)))
        return data
    }

    static func readHeader(_ recovery: URL) throws -> Header {
        let bad = EngineError(message: "“\(recovery.lastPathComponent)” isn't a usable recovery file.")
        let handle = try FileHandle(forReadingFrom: recovery)
        defer { try? handle.close() }
        guard let start = try handle.read(upToCount: magic.count + 20), start.count == magic.count + 20,
              start.prefix(magic.count) == magic else { throw bad }
        func get<T: FixedWidthInteger>(_ at: Int, _: T.Type) -> T {
            start.subdata(in: at..<at + MemoryLayout<T>.size).withUnsafeBytes { T(littleEndian: $0.loadUnaligned(as: T.self)) }
        }
        let size = get(magic.count, UInt64.self), slice = Int(get(magic.count + 8, UInt64.self))
        let k = Int(get(magic.count + 16, UInt16.self)), m = Int(get(magic.count + 18, UInt16.self))
        guard k > 0, m > 0, k + m <= 255, slice > 0 else { throw bad }
        try handle.seek(toOffset: 0)
        guard let all = try handle.read(upToCount: headerSize(k: k, m: m)), all.count == headerSize(k: k, m: m),
              Data(SHA256.hash(data: all.dropLast(32))) == all.suffix(32) else { throw bad }
        let hashes = all.dropFirst(magic.count + 20)
        let list = (0..<(k + m)).map { hashes.subdata(in: hashes.startIndex + $0 * 32 ..< hashes.startIndex + $0 * 32 + 32) }
        return Header(archiveSize: size, sliceSize: slice, dataHashes: Array(list.prefix(k)), parityHashes: Array(list.suffix(m)))
    }

    // MARK: Reed–Solomon

    /// Parity row j, data column i: 1 / (x_j + y_i), with x_j = k + j and y_i = i all
    /// distinct, so every square piece of it can be inverted and any m slices rebuilt.
    static func cauchy(k: Int, m: Int) -> [[UInt8]] {
        (0..<m).map { j in (0..<k).map { i in GF.inverse(UInt8(k + j) ^ UInt8(i)) } }
    }

    /// Gauss–Jordan elimination over GF(256).
    static func invert(_ matrix: [[UInt8]]) throws -> [[UInt8]] {
        let n = matrix.count
        var a = matrix
        var inv = (0..<n).map { r in (0..<n).map { c in UInt8(r == c ? 1 : 0) } }
        for col in 0..<n {
            guard let pivot = (col..<n).first(where: { a[$0][col] != 0 }) else {
                throw EngineError(message: "The recovery data couldn't be used.")
            }
            a.swapAt(col, pivot)
            inv.swapAt(col, pivot)
            let scale = GF.inverse(a[col][col])
            for c in 0..<n {
                a[col][c] = GF.mul(a[col][c], scale)
                inv[col][c] = GF.mul(inv[col][c], scale)
            }
            for r in 0..<n where r != col && a[r][col] != 0 {
                let factor = a[r][col]
                for c in 0..<n {
                    a[r][c] ^= GF.mul(factor, a[col][c])
                    inv[r][c] ^= GF.mul(factor, inv[col][c])
                }
            }
        }
        return inv
    }
}

/// Arithmetic in GF(256), with the polynomial x⁸ + x⁴ + x³ + x² + 1.
enum GF {
    private static let tables: (exp: [UInt8], log: [Int]) = {
        var exp = [UInt8](repeating: 0, count: 512), log = [Int](repeating: 0, count: 256)
        var x = 1
        for i in 0..<255 {
            exp[i] = UInt8(x)
            log[x] = i
            x <<= 1
            if x & 0x100 != 0 { x ^= 0x11D }
        }
        for i in 255..<512 { exp[i] = exp[i - 255] }
        return (exp, log)
    }()

    /// Every product, for the hot loop.
    private static let products: [UInt8] = {
        var table = [UInt8](repeating: 0, count: 256 * 256)
        for a in 1..<256 { for b in 1..<256 { table[a << 8 | b] = mul(UInt8(a), UInt8(b)) } }
        return table
    }()

    static func mul(_ a: UInt8, _ b: UInt8) -> UInt8 {
        guard a != 0, b != 0 else { return 0 }
        return tables.exp[tables.log[Int(a)] + tables.log[Int(b)]]
    }

    static func inverse(_ a: UInt8) -> UInt8 {
        precondition(a != 0)
        return tables.exp[255 - tables.log[Int(a)]]
    }

    /// out ^= c · input, byte by byte.
    static func mulAdd(_ out: inout [UInt8], _ input: [UInt8], _ c: UInt8) {
        out.withUnsafeMutableBufferPointer { o in input.withUnsafeBufferPointer { mulAdd(o, $0, c) } }
    }

    static func mulAdd(_ out: UnsafeMutableBufferPointer<UInt8>, _ input: UnsafeBufferPointer<UInt8>, _ c: UInt8) {
        guard c != 0 else { return }
        let n = min(out.count, input.count)
        if c == 1 {
            for x in 0..<n { out[x] ^= input[x] }
            return
        }
        let row = Int(c) << 8
        products.withUnsafeBufferPointer { table in
            for x in 0..<n { out[x] ^= table[row | Int(input[x])] }
        }
    }
}
