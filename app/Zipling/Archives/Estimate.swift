import Foundation

/// What an archive will weigh and how long it will take, from compressing a sample
/// spread across the files with the chosen settings. When everything fits in the
/// sample, the estimate is the real result.
enum Estimate {
    struct Result: Sendable, Equatable {
        let bytes: UInt64
        let seconds: Double
        /// True when the whole input was compressed, so these are the real numbers.
        let exact: Bool
    }

    /// Auto's pick of level for a Zipling archive, and why.
    struct AutoChoice: Sendable, Equatable {
        let level: Level
        let estimate: Result
        let reason: String
    }

    /// Picks a level from the sample: Fast when there's nothing left to squeeze,
    /// Max when it's clearly smaller and doesn't take too long, Balanced otherwise.
    static func auto(_ items: [URL], contents: ContentSummary, task: EngineTask, cancel: CancelToken) async throws -> AutoChoice {
        func estimate(_ level: Level) async throws -> Result {
            try await of(items, options: CreateOptions(format: .zlg, level: level), task: task, cancel: cancel)
        }
        if contents.share(contents.media) > 0.7 {
            return AutoChoice(level: .fast, estimate: try await estimate(.fast),
                              reason: "Mostly video, music or archives, which are already compressed: Fast, since slower levels would barely help.")
        }
        let balanced = try await estimate(.balanced)
        let max = try await estimate(.max)
        let smaller = balanced.bytes == 0 ? 0 : 1 - Double(max.bytes) / Double(balanced.bytes)
        let extra = Format.duration(Swift.max(0, max.seconds - balanced.seconds))
        if smaller >= 0.08, max.seconds <= 120 {
            return AutoChoice(level: .max, estimate: max,
                              reason: "Max: \(Format.percent(smaller)) smaller than Balanced, for about \(extra) more.")
        }
        return AutoChoice(level: .balanced, estimate: balanced, reason: smaller < 0.08
            ? "Balanced: Max would only be \(Format.percent(smaller)) smaller here."
            : "Balanced: Max would take about \(Format.duration(max.seconds)).")
    }

    /// How much is compressed to estimate.
    static let budget: UInt64 = 24 << 20
    /// Files bigger than this are left out of the sample and assumed to compress like the rest.
    static let largestSample: UInt64 = 8 << 20

    /// Every nth file in order, so the sample spans the whole set, up to the budget.
    static func pick<T>(_ files: [(T, UInt64)]) -> [(T, UInt64)] {
        let total = files.reduce(UInt64(0)) { $0 + $1.1 }
        guard total > budget else { return files }
        let candidates = files.filter { $0.1 <= largestSample }
        let candidateBytes = candidates.reduce(UInt64(0)) { $0 + $1.1 }
        guard candidateBytes > 0 else { return [] }
        let every = max(1, Int((Double(candidateBytes) / Double(budget)).rounded(.up)))
        return candidates.enumerated().filter { $0.offset % every == every / 2 }.map(\.element)
    }

    /// Files and their sizes under the items, in a stable order.
    static func files(under items: [URL]) -> [(URL, UInt64)] {
        var out: [(URL, UInt64)] = []
        let keys: Set<URLResourceKey> = [.fileSizeKey, .isRegularFileKey]
        for url in items {
            if let v = try? url.resourceValues(forKeys: keys), v.isRegularFile == true {
                out.append((url, UInt64(v.fileSize ?? 0)))
                continue
            }
            let walker = FileManager.default.enumerator(at: url, includingPropertiesForKeys: Array(keys))
            while let file = walker?.nextObject() as? URL {
                if let v = try? file.resourceValues(forKeys: keys), v.isRegularFile == true {
                    out.append((file, UInt64(v.fileSize ?? 0)))
                }
            }
        }
        return out.sorted { $0.0.path < $1.0.path }
    }

    /// Estimates compressing files on disk.
    static func of(_ items: [URL], options: CreateOptions, task: EngineTask, cancel: CancelToken) async throws -> Result {
        let all = files(under: items)
        let total = all.reduce(UInt64(0)) { $0 + $1.1 }
        let sample = pick(all)
        let work = FileManager.default.temporaryDirectory.appending(path: "Zipling/estimate-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: work) }
        let folder = work.appending(path: "sample")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        // APFS clones: nothing is actually copied.
        for (n, (url, _)) in sample.enumerated() {
            try FileManager.default.copyItem(at: url, to: folder.appending(path: "\(n) \(url.lastPathComponent)"))
        }
        let exact = sample.count == all.count
        return try await measure(folder, sampleBytes: sample.reduce(0) { $0 + $1.1 }, totalBytes: total, exact: exact,
                                 options: options, work: work, task: task, cancel: cancel)
    }

    /// Estimates re-making an archive's contents, extracting only the sample.
    static func of(_ archive: OpenedArchive, options: CreateOptions, task: EngineTask, cancel: CancelToken) async throws -> Result {
        let all = archive.listing.entries.filter { $0.kind == "file" }.map { ($0.path, $0.size) }
        let total = all.reduce(UInt64(0)) { $0 + $1.1 }
        let sample = pick(all)
        let work = FileManager.default.temporaryDirectory.appending(path: "Zipling/estimate-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: work) }
        let folder = work.appending(path: "sample")
        let exact = sample.count == all.count
        _ = try await Archives.extract(archive, to: folder, only: exact ? [] : sample.map(\.0), task: task,
                                       cancel: cancel) { _, _ in }
        return try await measure(folder, sampleBytes: sample.reduce(0) { $0 + $1.1 }, totalBytes: total, exact: exact,
                                 options: options, work: work, task: task, cancel: cancel)
    }

    private static func measure(
        _ folder: URL, sampleBytes: UInt64, totalBytes: UInt64, exact: Bool, options: CreateOptions, work: URL,
        task: EngineTask, cancel: CancelToken
    ) async throws -> Result {
        let items = SafeExtraction.names(in: folder.path).sorted().map { folder.appending(path: $0) }
        guard !items.isEmpty, sampleBytes > 0 else { return Result(bytes: 0, seconds: 0, exact: exact) }
        var options = options
        options.splitBytes = nil
        let started = ContinuousClock.now
        let made = try await Creation.create(
            items, into: work.appending(path: "sample.\(options.format.fileExtension)"), options: options,
            password: nil, task: task, cancel: cancel, check: false) { _, _ in }
        let elapsed = ContinuousClock.now - started
        let seconds = Double(elapsed.components.seconds) + Double(elapsed.components.attoseconds) / 1e18
        let out = made.files.reduce(UInt64(0)) { $0 + Conversion.size(of: $1) }
        let scale = Double(totalBytes) / Double(sampleBytes)
        return Result(bytes: UInt64(Double(out) * scale), seconds: seconds * scale, exact: exact)
    }
}
