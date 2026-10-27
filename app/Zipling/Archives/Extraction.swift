import AppKit
import Foundation

/// Gets files out of an archive and into place, for browser windows and batch jobs
/// alike. Everything is extracted into a hidden staging folder on the same volume,
/// checked and finished there, then moved into place: nothing is overwritten without
/// asking, and a cancelled or failed extraction leaves nothing behind.
@MainActor
enum Extraction {
    enum Layout {
        /// The selection, or everything, goes straight into the destination.
        case asIs
        /// Extract Here: one item at the top comes out as it is; several go in a new
        /// folder with this name (the archive's), so they don't spill everywhere.
        case smart(folder: String)
    }

    struct Outcome {
        let result: ExtractResult
        let placed: [URL]
        let removedLinks: Int
        /// Items left where they were because their name was taken and the choice was Skip.
        let conflictsSkipped: Int
        /// True when the whole archive came out with nothing left out.
        let complete: Bool

        var summary: String {
            if placed.isEmpty, conflictsSkipped > 0 {
                return "Nothing new came out: " + (conflictsSkipped == 1 ? "the item was already there and was kept."
                    : "all \(conflictsSkipped) items were already there and were kept.")
            }
            var text = "Extracted \(Format.count(result.files, "file")) (\(Format.bytes(result.bytes))) in \(Format.duration(result.seconds))."
            if let skipped = result.skipped, !skipped.isEmpty {
                text += " \(Format.count(UInt64(skipped.count), "item")) " + (skipped.count == 1 ? "was" : "were")
                    + " left out because writing " + (skipped.count == 1 ? "it" : "them") + " would have been unsafe."
            }
            if removedLinks > 0 {
                text += " \(Format.count(UInt64(removedLinks), "link")) pointing outside the archive "
                    + (removedLinks == 1 ? "was" : "were") + " left out for safety."
            }
            if conflictsSkipped > 0 {
                text += " \(Format.count(UInt64(conflictsSkipped), "item")) already there " + (conflictsSkipped == 1 ? "was" : "were") + " kept."
            }
            return text
        }
    }

    static func run(
        _ archive: OpenedArchive, only roots: [String], to dest: URL, layout: Layout, job: Job,
        quarantine: Data?, conflicts: Conflicts
    ) async throws -> Outcome {
        let fm = FileManager.default
        try fm.createDirectory(at: dest, withIntermediateDirectories: true)
        let staging = dest.appending(path: ".zipling-extract-\(UUID().uuidString)")
        defer { try? fm.removeItem(at: staging) }

        let result = try await Archives.extract(
            archive, to: staging, only: roots, task: job.task, cancel: job.cancelToken, progress: job.progressHandler)
        let report = try SafeExtraction.finalize(staging, quarantine: quarantine)

        // A selection: those items. Everything: whatever actually landed in staging.
        var sources = (roots.isEmpty ? SafeExtraction.names(in: staging.path).sorted() : roots)
            .map { staging.appending(path: $0) }
            .filter(Placement.exists)
        if case .smart(let folder) = layout, sources.count > 1 {
            let group = staging.appending(path: ".zipling-group-\(UUID().uuidString)")
                .appending(path: folder.isEmpty ? "Archive" : folder)
            try fm.createDirectory(at: group, withIntermediateDirectories: true)
            for source in sources { try fm.moveItem(at: source, to: group.appending(path: source.lastPathComponent)) }
            sources = [group]
        }

        var placed: [URL] = []
        var kept = 0
        for source in sources {
            let name = source.lastPathComponent
            var target = dest.appending(path: name)
            if Placement.exists(target) {
                switch await conflicts.resolve(name: name, in: dest) {
                case .keepBoth, .ask:
                    target = Placement.unique(name, in: dest)
                case .replace:
                    // Into the Trash, not gone: a wrong click can be undone.
                    try fm.trashItem(at: target, resultingItemURL: nil)
                case .skip:
                    kept += 1
                    continue
                }
            }
            try fm.moveItem(at: source, to: target)
            placed.append(target)
        }
        let complete = roots.isEmpty && (result.skipped ?? []).isEmpty && report.removedLinks.isEmpty && kept == 0
        return Outcome(result: result, placed: placed, removedLinks: report.removedLinks.count,
                       conflictsSkipped: kept, complete: complete)
    }

    /// Formats that keep checksums of the data itself, so a clean extraction proves
    /// every byte came out right. Plain tar, cpio and disk filesystems don't.
    static func checksDataOf(_ archive: OpenedArchive) -> Bool {
        if archive.backend == .arca { return true }
        let format = (archive.listing.format ?? "").lowercased()
        let unchecked = ["tar", "cpio", "ar", "iso", "udf", "hfs", "apfs", "fat", "ntfs", "ext", "squashfs", "macbinary", "binhex"]
        return !unchecked.contains(format)
    }

    /// Moves an archive, and every volume of a split one, to the Trash.
    static func trash(_ url: URL) throws -> [URL] {
        let volumes = Archives.volumes(of: url)
        for volume in volumes { try FileManager.default.trashItem(at: volume, resultingItemURL: nil) }
        return volumes
    }
}

/// Asks what to do when a name is taken, remembering "do this for every conflict".
@MainActor
final class Conflicts {
    private let policy: Preferences.Conflict
    private var remembered: Preferences.Conflict?

    init(policy: Preferences.Conflict = Preferences.conflict) {
        self.policy = policy
    }

    func resolve(name: String, in folder: URL) async -> Preferences.Conflict {
        if policy != .ask { return policy }
        if let remembered { return remembered }
        let alert = NSAlert()
        alert.messageText = "“\(name)” already exists in “\(folder.lastPathComponent)”."
        alert.informativeText = "Keep both to give the new one a numbered name. Replacing moves the existing item to the Trash."
        alert.addButton(withTitle: "Keep Both")
        alert.addButton(withTitle: "Replace")
        alert.addButton(withTitle: "Skip")
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title = "Do this for every conflict in this extraction"
        let choice: Preferences.Conflict = switch alert.runModal() {
        case .alertSecondButtonReturn: .replace
        case .alertThirdButtonReturn: .skip
        default: .keepBoth
        }
        if alert.suppressionButton?.state == .on { remembered = choice }
        return choice
    }
}

/// Extract Here for several archives at once (from the menu, Finder or the Dock),
/// one after another in the background, each next to itself.
@MainActor
enum BatchExtraction {
    static func run(_ urls: [URL], conflicts: Conflicts = Conflicts(), reveal: Bool = Preferences.revealAfterExtract) async {
        var placed: [URL] = []
        for url in urls {
            let job = Job(title: "Extracting “\(url.lastPathComponent)”…")
            JobCenter.shared.track(job)
            let scratch = FileManager.default.temporaryDirectory.appending(path: "Zipling/\(UUID().uuidString)")
            defer { try? FileManager.default.removeItem(at: scratch) }
            do {
                var password: String?
                var archive: OpenedArchive
                while true {
                    do {
                        archive = try await Archives.open(url, password: password, scratch: scratch, cancel: job.cancelToken)
                        break
                    } catch let p as PasswordRequired {
                        password = try askPassword(for: url.lastPathComponent, wrong: p.wrong)
                    }
                }
                var outcome: Extraction.Outcome
                while true {
                    do {
                        outcome = try await Extraction.run(
                            archive, only: [], to: url.deletingLastPathComponent(),
                            layout: .smart(folder: Archives.baseName(url.lastPathComponent)), job: job,
                            quarantine: SafeExtraction.quarantine(of: url), conflicts: conflicts)
                        break
                    } catch let p as PasswordRequired {
                        archive.password = try askPassword(for: url.lastPathComponent, wrong: p.wrong)
                    }
                }
                var summary = outcome.summary
                if outcome.complete, Preferences.trashAfterExtract, Extraction.checksDataOf(archive),
                   let trashed = try? Extraction.trash(url) {
                    summary += trashed.count > 1
                        ? " Its \(trashed.count) parts were moved to the Trash."
                        : " The archive was moved to the Trash."
                }
                placed += outcome.placed
                job.finish(nil, outcome: summary, placed: outcome.placed)
            } catch {
                job.finish(error)
            }
        }
        if reveal, !placed.isEmpty { NSWorkspace.shared.activateFileViewerSelecting(placed) }
    }

    /// No window to show a sheet in, so a plain alert asks.
    private static func askPassword(for name: String, wrong: Bool) throws -> String {
        let alert = NSAlert()
        alert.messageText = "“\(name)” is encrypted"
        alert.informativeText = wrong ? "That password didn't work. Try again." : "Enter its password to extract it."
        let field = NSSecureTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
        alert.accessoryView = field
        alert.addButton(withTitle: "Extract")
        alert.addButton(withTitle: "Skip")
        alert.window.initialFirstResponder = field
        guard alert.runModal() == .alertFirstButtonReturn, !field.stringValue.isEmpty else {
            throw EngineError(message: "cancelled")
        }
        return field.stringValue
    }
}
