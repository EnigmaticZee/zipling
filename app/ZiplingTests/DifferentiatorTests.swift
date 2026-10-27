import CoreSpotlight
import Foundation
import Testing
@testable import Zipling

/// Stage 8: converting archives, the size and time estimate with Auto, shrinking to
/// Max later, and recovery files that repair damage.
@MainActor
struct DifferentiatorTests {
    private let fm = FileManager.default

    /// Text that compresses well, a little noise, and a nested folder.
    private func makeTree() throws -> URL {
        let root = fm.temporaryDirectory.appending(path: "DifferentiatorTests-\(UUID().uuidString)")
        let project = root.appending(path: "Project")
        try fm.createDirectory(at: project.appending(path: "docs/nested"), withIntermediateDirectories: true)
        var text = ""
        for n in 0..<40_000 { text += "line \(n): the quick brown fox jumps over the lazy dog\n" }
        try text.write(to: project.appending(path: "docs/log.txt"), atomically: true, encoding: .utf8)
        try "copy".write(to: project.appending(path: "docs/nested/copy.txt"), atomically: true, encoding: .utf8)
        var random = SystemRandomNumberGenerator()
        try Data((0..<120_000).map { _ in UInt8.random(in: 0...255, using: &random) })
            .write(to: project.appending(path: "noise.bin"))
        return root
    }

    private func make(_ root: URL, _ name: String, _ options: CreateOptions) async throws -> URL {
        try await Creation.create([root.appending(path: "Project")], into: root.appending(path: name), options: options,
                                  password: nil, task: EngineTask(), cancel: CancelToken()) { _, _ in }.files[0]
    }

    private func paths(_ archive: URL, in root: URL) async throws -> Set<String> {
        let opened = try await Archives.open(archive, password: nil, scratch: root.appending(path: "scratch"))
        return Set(opened.listing.entries.filter { !$0.isDirectory }.map(\.path))
    }

    // MARK: Converting

    @Test func convertsBetweenFormatsKeepingTheFiles() async throws {
        let root = try makeTree()
        let zip = try await make(root, "Project.zip", CreateOptions(format: .zip, level: .fast))
        let opened = try await Archives.open(zip, password: nil, scratch: root.appending(path: "scratch"))
        let converted = try await Conversion.convert(
            opened, from: zip, options: CreateOptions(format: .zlg, level: .balanced), password: nil, quarantine: nil,
            task: EngineTask(), cancel: CancelToken()) { _, _ in }
        let zlg = try #require(converted.made.files.first)
        #expect(zlg.lastPathComponent == "Project.zlg")
        #expect(Archives.isZipling(zlg))
        #expect(converted.after < converted.before, "the Zipling archive is smaller than a Fast ZIP")
        #expect(try await paths(zlg, in: root) == paths(zip, in: root), "the same files")

        // And back out to 7z, for someone without Zipling.
        let zlgOpened = try await Archives.open(zlg, password: nil, scratch: root.appending(path: "scratch"))
        let sevenZip = try await Conversion.convert(
            zlgOpened, from: zlg, options: CreateOptions(format: .sevenZip, level: .max), password: nil, quarantine: nil,
            task: EngineTask(), cancel: CancelToken()) { _, _ in }
        #expect(try await paths(sevenZip.made.files[0], in: root) == paths(zip, in: root))
        #expect(try fm.contentsOfDirectory(atPath: root.path).filter { $0.hasPrefix(".zipling") }.isEmpty, "nothing left behind")
    }

    // MARK: Estimating

    @Test func smallJobsAreEstimatedExactly() async throws {
        let root = try makeTree()
        let options = CreateOptions(format: .zlg, level: .balanced)
        let estimate = try await Estimate.of([root.appending(path: "Project")], options: options, task: EngineTask(),
                                             cancel: CancelToken())
        #expect(estimate.exact, "under the sample budget, everything is compressed")
        let real = Conversion.size(of: try await make(root, "Real.zlg", options))
        let error = abs(Double(estimate.bytes) - Double(real)) / Double(real)
        #expect(error < 0.05, "estimate \(estimate.bytes) vs real \(real)")
    }

    @Test func samplesSpanTheWholeSet() {
        // 400 files of 1 MB: the sample is about the budget, spread from start to end.
        let files = (0..<400).map { ($0, UInt64(1 << 20)) }
        let sample = Estimate.pick(files)
        let bytes = sample.reduce(UInt64(0)) { $0 + $1.1 }
        #expect(bytes <= Estimate.budget && bytes >= Estimate.budget / 2)
        #expect((sample.first?.0 ?? 0) < 20 && (sample.last?.0 ?? 0) > 380)
        // Files too big to sample are left out.
        #expect(Estimate.pick([(0, Estimate.budget * 2), (1, 1 << 20)]).map(\.0) == [1])
    }

    @Test func optionsAlwaysEncodeTheSame() {
        // The Compress sheet re-estimates whenever this text changes.
        let options = CreateOptions(format: .zlg, level: .max, splitBytes: 1 << 20, autoLevel: true)
        #expect(Set((0..<50).map { _ in options.encoded }).count == 1)
        #expect(CreateOptions(encoded: options.encoded) == options)
    }

    @Test func autoPicksFastForMediaAndMeasuresTheRest() async throws {
        let root = try makeTree()
        var media = ContentSummary(files: 1, bytes: 100)
        media.media = 90
        let fast = try await Estimate.auto([root.appending(path: "Project")], contents: media, task: EngineTask(),
                                           cancel: CancelToken())
        #expect(fast.level == .fast)
        let text = ContentSummary(files: 3, bytes: 100)
        let measured = try await Estimate.auto([root.appending(path: "Project")], contents: text, task: EngineTask(),
                                               cancel: CancelToken())
        #expect(measured.level == .balanced || measured.level == .max)
        #expect(measured.reason.hasPrefix(measured.level == .max ? "Max" : "Balanced"))
    }

    // MARK: Shrinking later

    @Test func shrinksToMaxKeepingTheFiles() async throws {
        let root = try makeTree()
        let archive = try await make(root, "Quick.zlg", CreateOptions(format: .zlg, level: .fast))
        let before = try await Engine.list(archive)
        let sizes = try await ShrinkLater.shrink(archive, task: EngineTask(), cancel: CancelToken()) { _, _ in }
        let (was, now) = try #require(sizes, "Max is smaller than Fast on text")
        #expect(now < was && Conversion.size(of: archive) == now)
        let after = try await Engine.list(archive)
        #expect(Set(after.entries.map(\.path)) == Set(before.entries.map(\.path)) && after.totalSize == before.totalSize)
        _ = try await Engine.verify(archive, task: EngineTask()) { _, _ in }
        #expect(try fm.contentsOfDirectory(atPath: root.path).filter { $0.hasPrefix(".zipling") }.isEmpty)
    }

    @Test func theShrinkQueueSkipsArchivesThatChanged() async throws {
        let root = try makeTree()
        let archive = try await make(root, "Quick.zlg", CreateOptions(format: .zlg, level: .fast))
        let suite = "DifferentiatorTests-\(UUID().uuidString)"
        let queue = ShrinkLater()
        queue.defaults = try #require(UserDefaults(suiteName: suite))
        defer { UserDefaults.standard.removePersistentDomain(forName: suite) }
        queue.add(archive)
        #expect(queue.queue.count == 1)
        // Someone changes it before the Mac is idle: it's left alone.
        try Data("changed".utf8).write(to: archive)
        await queue.shrinkAll()
        #expect(queue.queue.isEmpty)
        #expect(try Data(contentsOf: archive) == Data("changed".utf8))
    }

    // MARK: Recovery files

    @Test(arguments: [0.05, 0.10, 0.20])
    func recoveryFilesRepairDamage(_ redundancy: Double) async throws {
        let root = try makeTree()
        let archive = try await make(root, "Safe.zip", CreateOptions(format: .zip, level: .fast))
        let original = try Data(contentsOf: archive)
        let recovery = try await Recovery.create(for: archive, redundancy: redundancy, cancel: CancelToken()) { _, _ in }
        #expect(recovery.lastPathComponent == "Safe.zip.recovery")
        let header = try Recovery.readHeader(recovery)
        #expect(header.k + header.m <= 255)
        #expect(Double(header.m) >= Double(header.k) * redundancy - 1)
        #expect(try await Recovery.check(archive, recovery: recovery, cancel: CancelToken()).intact)

        // Damage: junk in two places, and the end cut off.
        var damaged = original
        let slice = header.sliceSize
        for at in [slice / 2, 3 * slice + 10] where at + 100 < damaged.count {
            damaged.replaceSubrange(at..<at + 100, with: Data(repeating: 0xAA, count: 100))
        }
        damaged.removeLast(min(damaged.count / 50, slice))
        try damaged.write(to: archive)
        let report = try await Recovery.check(archive, recovery: recovery, cancel: CancelToken())
        #expect(!report.intact && report.repairable, "\(report.damagedSlices) damaged, \(report.repairableSlices) rebuildable")

        let repaired = root.appending(path: "Safe (Repaired).zip")
        try await Recovery.repair(archive, recovery: recovery, to: repaired, cancel: CancelToken()) { _, _ in }
        #expect(try Data(contentsOf: repaired) == original, "bit for bit")
        _ = try await Archives.verify(
            try await Archives.open(repaired, password: nil, scratch: root.appending(path: "scratch")),
            task: EngineTask(), cancel: CancelToken()) { _, _ in }
    }

    @Test func archivesReadOnlyInPartAreMarkedDamaged() async throws {
        guard Unar.isAvailable else { return }
        let root = try makeTree()
        let archive = try await make(root, "Partly.zip", CreateOptions(format: .zip, level: .fast))
        let scratch = root.appending(path: "scratch")
        #expect(try await !Archives.open(archive, password: nil, scratch: scratch).salvaged)
        // Junk over the central directory at the end: 7-Zip gives up, The Unarchiver
        // still finds the files by their local headers.
        var data = try Data(contentsOf: archive)
        let tail = min(400, data.count / 4)
        data.replaceSubrange(data.count - tail..<data.count, with: Data(repeating: 0x5A, count: tail))
        try data.write(to: archive)
        let opened = try await Archives.open(archive, password: nil, scratch: scratch)
        #expect(opened.salvaged && opened.backend == .unar)
    }

    @Test func tooMuchDamageIsReportedNotGuessed() async throws {
        let root = try makeTree()
        let archive = try await make(root, "Broken.zip", CreateOptions(format: .zip, level: .fast))
        let recovery = try await Recovery.create(for: archive, redundancy: 0.05, cancel: CancelToken()) { _, _ in }
        var data = try Data(contentsOf: archive)
        data.replaceSubrange(0..<data.count / 2, with: Data(repeating: 0, count: data.count / 2))
        try data.write(to: archive)
        let report = try await Recovery.check(archive, recovery: recovery, cancel: CancelToken())
        #expect(!report.repairable)
        let output = root.appending(path: "Broken (Repaired).zip")
        await #expect(throws: EngineError.self) {
            try await Recovery.repair(archive, recovery: recovery, to: output, cancel: CancelToken()) { _, _ in }
        }
        #expect(!fm.fileExists(atPath: output.path), "nothing written")
    }

    @Test func reedSolomonRebuildsAnySlices() throws {
        // Every way of losing two of four data slices, rebuilt from two parity slices.
        let k = 4, m = 2, length = 64
        var random = SystemRandomNumberGenerator()
        let data = (0..<k).map { _ in (0..<length).map { _ in UInt8.random(in: 0...255, using: &random) } }
        let matrix = Recovery.cauchy(k: k, m: m)
        let parity = (0..<m).map { j in
            var out = [UInt8](repeating: 0, count: length)
            for i in 0..<k { GF.mulAdd(&out, data[i], matrix[j][i]) }
            return out
        }
        for a in 0..<k {
            for b in (a + 1)..<k {
                let erased = [a, b]
                let inverse = try Recovery.invert((0..<m).map { j in erased.map { matrix[j][$0] } })
                var syndromes = parity
                for i in 0..<k where !erased.contains(i) {
                    for j in 0..<m { GF.mulAdd(&syndromes[j], data[i], matrix[j][i]) }
                }
                for (c, index) in erased.enumerated() {
                    var rebuilt = [UInt8](repeating: 0, count: length)
                    for j in 0..<m { GF.mulAdd(&rebuilt, syndromes[j], inverse[c][j]) }
                    #expect(rebuilt == data[index], "slice \(index) of \(erased)")
                }
            }
        }
    }

    // MARK: Automation

    @Test func automationCompressesExtractsAndConverts() async throws {
        let root = try makeTree()
        let project = root.appending(path: "Project")
        let zip = try await Automation.compress([project], format: .zip, level: .fast)
        #expect(zip.lastPathComponent == "Project.zip")
        let auto = try await Automation.compress([project], format: .zlg, level: nil)
        #expect(Archives.isZipling(auto), "Auto made a Zipling archive")

        let out = root.appending(path: "out")
        let placed = try await Automation.extract(zip, into: out)
        #expect(placed.map(\.lastPathComponent) == ["Project"])
        #expect(fm.fileExists(atPath: out.appending(path: "Project/docs/log.txt").path))

        let converted = try await Automation.convert(zip, to: .sevenZip, level: .max)
        #expect(converted.pathExtension == "7z")
        #expect(try await paths(converted, in: root) == paths(zip, in: root))
    }

    @Test func folderActionScriptsCompile() throws {
        let folder = fm.temporaryDirectory.appending(path: "DifferentiatorTests-\(UUID().uuidString)")
        let installed = try Automation.installFolderActions(into: folder)
        let scripts = try fm.contentsOfDirectory(atPath: installed.path).sorted()
        #expect(scripts == ["Zipling – Compress Added Items.scpt", "Zipling – Extract Added Archives.scpt"])
    }

    @Test func spotlightKnowsWhatsInside() async throws {
        let root = try makeTree()
        let zip = try await make(root, "Spot.zip", CreateOptions(format: .zip, level: .fast))
        let opened = try await Archives.open(zip, password: nil, scratch: root.appending(path: "scratch"))
        let item = Spotlight.item(for: zip, listing: opened.listing)
        #expect(item.uniqueIdentifier == zip.path)
        #expect(item.attributeSet.title == "Spot.zip")
        #expect(item.attributeSet.textContent?.contains("Project/docs/nested/copy.txt") == true)
        #expect(item.attributeSet.keywords?.contains("log.txt") == true)
        // Opening a Spotlight result opens the archive.
        let activity = NSUserActivity(activityType: CSSearchableItemActionType)
        activity.userInfo = [CSSearchableItemActivityIdentifier: zip.path]
        #expect(Spotlight.archive(for: activity) == zip)
    }

    /// Making a recovery file reads the archive once; repairing it is about as quick.
    @Test func recoveryFilesAreQuickToMake() async throws {
        let root = fm.temporaryDirectory.appending(path: "DifferentiatorTests-\(UUID().uuidString)")
        try fm.createDirectory(at: root, withIntermediateDirectories: true)
        let archive = root.appending(path: "Big.bin")
        var random = SystemRandomNumberGenerator()
        var block = Data(count: 1 << 20)
        block.withUnsafeMutableBytes { for i in 0..<$0.count { $0[i] = UInt8.random(in: 0...255, using: &random) } }
        fm.createFile(atPath: archive.path, contents: nil)
        let handle = try FileHandle(forWritingTo: archive)
        for _ in 0..<32 { try handle.write(contentsOf: block) }
        try handle.close()
        let clock = ContinuousClock()
        let made = try await clock.measure {
            try await Recovery.create(for: archive, redundancy: 0.10, cancel: CancelToken()) { _, _ in }
        }
        let seconds = Double(made.components.seconds) + Double(made.components.attoseconds) / 1e18
        print("recovery file for 32 MB at 10%: \(made), \(Int(32 / seconds)) MB/s")
        #expect(made < .seconds(120))
    }
}
