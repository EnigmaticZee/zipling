import SwiftUI

/// Sheet for turning the archive on screen into another format: a smaller Zipling
/// archive, or a tighter ZIP or 7z that opens anywhere. The contents stay the same.
struct ConvertView: View {
    let model: ArchiveModel

    @Environment(\.dismiss) private var dismiss
    @State private var options = CreateOptions(format: .zlg, level: .balanced, leaveOutMacFiles: false)
    @State private var trashOriginal = false
    @State private var estimate: Estimate.Result?
    @State private var job: Job?
    @State private var result: Conversion.Result?
    @State private var estimateFailed = false

    /// The window's own archive, with its password once unlocked.
    private var archive: OpenedArchive { model.levels[0].archive }
    private var locked: Bool { encrypted && archive.password == nil }

    private var formats: [ArchiveFormat] { ArchiveFormat.allCases }
    private var encrypted: Bool { archive.listing.encrypted == true }
    /// Encrypted archives stay encrypted as ZIP or 7z, with the same password.
    private var keepsPassword: Bool { encrypted && options.format.supportsPassword && archive.password != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            if let job, let result, case .finished = job.phase {
                summary(result)
            } else if let job {
                JobProgressView(job: job)
            } else {
                choices
            }
            footer
        }
        .padding(24)
        .frame(width: 560)
        .task(id: "\(options.encoded)|\(archive.password != nil)") { await updateEstimate() }
    }

    private var header: some View {
        HStack(spacing: 14) {
            Image(nsImage: NSWorkspace.shared.icon(forFile: model.url.path))
                .resizable()
                .frame(width: 48, height: 48)
            VStack(alignment: .leading, spacing: 3) {
                Text("Convert “\(model.url.lastPathComponent)”").font(.title3.weight(.semibold)).lineLimit(1)
                Text("\((archive.listing.format ?? "Zipling").uppercased()) · \(Format.count(archive.listing.files, "file")) · \(Format.bytes(Conversion.size(of: model.url)))")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder private var choices: some View {
        VStack(alignment: .leading, spacing: 6) {
            Picker("Format", selection: $options.format) {
                ForEach(formats) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Text(options.format.blurb).font(.callout).foregroundStyle(.secondary)
        }
        if options.hasLevels {
            Picker("Compression", selection: $options.level) {
                ForEach(Level.allCases) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
        }
        if locked {
            HStack {
                Label("It's encrypted: unlock it to convert it.", systemImage: "lock.fill")
                    .font(.callout)
                Spacer()
                Button("Unlock…") { model.unlock {} }
            }
        } else {
            EstimateLine(estimate: estimate, total: Conversion.size(of: model.url), unavailable: estimateFailed)
                .help("Compared with the archive as it is now")
        }
        if encrypted, !locked {
            Label(keepsPassword ? "Stays encrypted with the same password."
                  : "\(options.format.title) archives can't be encrypted yet, so the converted archive won't be.",
                  systemImage: keepsPassword ? "lock.fill" : "exclamationmark.triangle")
                .font(.callout)
                .foregroundStyle(keepsPassword ? AnyShapeStyle(.secondary) : AnyShapeStyle(.orange))
        }
        Toggle("Move the original to the Trash once the new one has been checked", isOn: $trashOriginal)
            .font(.callout)
    }

    private func summary(_ result: Conversion.Result) -> some View {
        let saved = result.before == 0 ? 0 : 1 - Double(result.after) / Double(result.before)
        return HStack(alignment: .top, spacing: 24) {
            SavingsRing(fraction: max(0, saved), size: 96)
            VStack(alignment: .leading, spacing: 8) {
                Text("\(Format.bytes(result.before)) → \(Format.bytes(result.after))")
                    .font(.title3.weight(.semibold))
                    .monospacedDigit()
                Text(saved >= 0 ? "\(Format.percent(saved)) smaller than before, with the same files."
                     : "\(Format.percent(-saved)) larger than before, with the same files.")
                    .foregroundStyle(.secondary)
                Label("Read back and checked after writing", systemImage: "checkmark.circle.fill")
                    .symbolRenderingMode(.multicolor)
                    .font(.callout)
                if trashOriginal {
                    Label("The original was moved to the Trash", systemImage: "trash")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    @ViewBuilder private var footer: some View {
        HStack {
            if let job, let result, case .finished = job.phase {
                Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting(result.made.files) }
                Button("Open") {
                    AppRouter.shared.open([result.made.files[0]])
                    dismiss()
                }
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.defaultAction)
            } else if let job, job.isRunning {
                Spacer()
                Button("Cancel", role: .cancel) { job.cancel() }.keyboardShortcut(.cancelAction)
            } else {
                if let job, case .failed(let message) = job.phase {
                    Label(message, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red).lineLimit(2)
                } else if let job, case .cancelled = job.phase {
                    Label("Cancelled. Nothing was written.", systemImage: "xmark.circle").foregroundStyle(.secondary)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }.keyboardShortcut(.cancelAction)
                Button(job == nil ? "Convert" : "Try Again") { start() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(locked)
            }
        }
        .controlSize(.large)
    }

    private func updateEstimate() async {
        guard job == nil, !locked else { return }
        estimate = nil
        estimateFailed = false
        try? await Task.sleep(for: .milliseconds(350))
        if Task.isCancelled { return }
        let task = EngineTask(), cancel = CancelToken()
        let (archive, options) = (archive, options)
        do {
            let result = try await withTaskCancellationHandler {
                try await Estimate.of(archive, options: options, task: task, cancel: cancel)
            } onCancel: {
                task.cancel()
                cancel.cancel()
            }
            if !Task.isCancelled { estimate = result }
        } catch {
            if !Task.isCancelled { estimateFailed = true }
        }
    }

    private func start() {
        let job = Job(title: "Converting to \(options.format.title)…")
        self.job = job
        result = nil
        JobCenter.shared.track(job)
        let (archive, original, options, trash) = (archive, model.url, options, trashOriginal)
        let password = keepsPassword ? archive.password : nil
        let quarantine = SafeExtraction.quarantine(of: original)
        Task {
            do {
                let converted = try await Conversion.convert(
                    archive, from: original, options: options, password: password, quarantine: quarantine,
                    task: job.task, cancel: job.cancelToken, progress: job.progressHandler)
                if trash { _ = try? Extraction.trash(original) }
                result = converted
                job.finish(nil, outcome: "Made \(converted.made.files.map(\.lastPathComponent).joined(separator: ", ")): \(Format.bytes(converted.before)) → \(Format.bytes(converted.after)).",
                           placed: converted.made.files)
                if let first = converted.made.files.first { AppRouter.shared.noteCreated(first) }
            } catch {
                job.finish(error)
            }
        }
    }
}
