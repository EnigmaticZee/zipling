import SwiftUI

extension Level {
    var title: String {
        switch self {
        case .fast: "Fast"
        case .balanced: "Balanced"
        case .max: "Max"
        case .ultra: "Ultra"
        }
    }

    var symbol: String {
        switch self {
        case .fast: "hare"
        case .balanced: "gauge.with.dots.needle.50percent"
        case .max: "arrow.down.right.and.arrow.up.left"
        case .ultra: "sparkles"
        }
    }

    var blurb: String {
        switch self {
        case .fast: "Done in seconds. Smaller than a Finder zip."
        case .balanced: "Smaller than 7-Zip Ultra, and several times faster."
        case .max: "Smallest archive that still opens quickly."
        case .ultra: "Every last byte. Slower to open."
        }
    }
}

/// Sheet for compressing files into a new archive.
struct CompressView: View {
    let items: [URL]

    @Environment(\.dismiss) private var dismiss
    @Environment(\.openWindow) private var openWindow
    @AppStorage("compressLevel") private var level: Level = .balanced
    @State private var output: URL
    @State private var inputSize: UInt64?
    @State private var job: Job?
    @State private var result: PackResult?

    init(items: [URL]) {
        self.items = items
        _output = State(initialValue: Self.defaultOutput(for: items))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            header
            if let job, let result, case .finished = job.phase {
                ResultSummary(result: result)
            } else if let job {
                JobProgressView(job: job)
            } else {
                levelPicker
                destination
            }
            footer
        }
        .padding(24)
        .frame(width: 620)
        .task { inputSize = await Self.measure(items) }
    }

    // MARK: Sections

    private var header: some View {
        HStack(spacing: 14) {
            ZStack {
                ForEach(Array(items.prefix(3).enumerated().reversed()), id: \.offset) { i, url in
                    Image(nsImage: NSWorkspace.shared.icon(forFile: url.path))
                        .resizable()
                        .frame(width: 48, height: 48)
                        .offset(x: CGFloat(i) * 6, y: CGFloat(-i) * 4)
                }
            }
            .frame(width: 64, height: 56)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.title3.weight(.semibold)).lineLimit(1)
                Text(subtitle).font(.callout).foregroundStyle(.secondary)
            }
        }
    }

    private var title: String {
        items.count == 1 ? "Compress “\(items[0].lastPathComponent)”" : "Compress \(items.count) items"
    }

    private var subtitle: String {
        guard let inputSize else { return "Measuring…" }
        return "\(Format.bytes(inputSize)) · lossless, verified before anything is written"
    }

    private var levelPicker: some View {
        HStack(spacing: 10) {
            ForEach(Level.allCases) { option in
                LevelCard(level: option, selected: option == level, recommended: option == .balanced) {
                    level = option
                }
            }
        }
    }

    private var destination: some View {
        HStack(spacing: 10) {
            Image(nsImage: NSWorkspace.shared.icon(for: .ziplingArchive))
                .resizable()
                .frame(width: 22, height: 22)
            VStack(alignment: .leading, spacing: 1) {
                Text(output.lastPathComponent).lineLimit(1)
                Text(output.deletingLastPathComponent().path(percentEncoded: false))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            Spacer()
            Button("Change…") { chooseOutput() }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(.quaternary.opacity(0.5)))
    }

    @ViewBuilder private var footer: some View {
        HStack {
            if let job, let result, case .finished = job.phase {
                Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([output]) }
                Button("Open Archive") {
                    AppRouter.shared.open([output])
                    dismiss()
                }
                Spacer()
                Text("\(Format.bytes(result.outputBytes)) in \(Format.duration(result.seconds))")
                    .foregroundStyle(.secondary)
                Button("Done") { dismiss() }
                    .keyboardShortcut(.defaultAction)
            } else if let job, job.isRunning {
                Spacer()
                Button("Cancel", role: .cancel) { job.cancel() }
                    .keyboardShortcut(.cancelAction)
            } else {
                if let job, case .failed(let message) = job.phase {
                    Label(message, systemImage: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                        .lineLimit(2)
                } else if let job, case .cancelled = job.phase {
                    Label("Cancelled. Nothing was written.", systemImage: "xmark.circle")
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(job == nil ? "Compress" : "Try Again") { start() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
            }
        }
        .controlSize(.large)
    }

    // MARK: Actions

    private func start() {
        let job = Job(title: "Compressing with \(level.title)…")
        self.job = job
        result = nil
        let (items, output, level) = (items, output, level)
        Task {
            do {
                let r = try await Engine.pack(items, into: output, level: level, task: job.task, progress: job.progressHandler)
                result = r
                job.finish(nil)
                AppRouter.shared.noteCreated(output)
            } catch {
                job.finish(error)
            }
        }
    }

    private func chooseOutput() {
        let panel = NSSavePanel()
        panel.allowedContentTypes = [.ziplingArchive]
        panel.directoryURL = output.deletingLastPathComponent()
        panel.nameFieldStringValue = output.lastPathComponent
        panel.canCreateDirectories = true
        if panel.runModal() == .OK, let url = panel.url { output = url }
    }

    // MARK: Helpers

    private static func defaultOutput(for items: [URL]) -> URL {
        let folder = items.first?.deletingLastPathComponent() ?? URL.homeDirectory
        let base = items.count == 1 ? items[0].deletingPathExtension().lastPathComponent : "Archive"
        return Placement.unique("\(base).zlg", in: folder)
    }

    /// Total size of the items, walking folders off the main thread.
    private static func measure(_ items: [URL]) async -> UInt64 {
        await Task.detached(priority: .utility) {
            var total: UInt64 = 0
            let keys: [URLResourceKey] = [.fileSizeKey, .isRegularFileKey]
            for url in items {
                if let v = try? url.resourceValues(forKeys: Set(keys)), v.isRegularFile == true {
                    total += UInt64(v.fileSize ?? 0)
                    continue
                }
                let walker = FileManager.default.enumerator(at: url, includingPropertiesForKeys: keys)
                while let file = walker?.nextObject() as? URL {
                    if let v = try? file.resourceValues(forKeys: Set(keys)), v.isRegularFile == true {
                        total += UInt64(v.fileSize ?? 0)
                    }
                }
            }
            return total
        }.value
    }
}

private struct LevelCard: View {
    let level: Level
    let selected: Bool
    let recommended: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Image(systemName: level.symbol)
                        .font(.title3)
                        .foregroundStyle(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                        .frame(height: 22)
                    Spacer()
                    if recommended {
                        Text("Default")
                            .font(.caption2.weight(.semibold))
                            .padding(.horizontal, 6)
                            .padding(.vertical, 2)
                            .background(Capsule().fill(.tint.opacity(0.15)))
                            .foregroundStyle(.tint)
                    }
                }
                Text(level.title).font(.headline)
                Text(level.blurb)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(12)
            .frame(maxWidth: .infinity, minHeight: 124, alignment: .topLeading)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(selected ? AnyShapeStyle(.tint.opacity(0.10)) : AnyShapeStyle(.quaternary.opacity(0.4)))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .strokeBorder(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.separator), lineWidth: selected ? 2 : 1)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .animation(.snappy(duration: 0.15), value: selected)
    }
}

/// What the archive saved, and where the savings came from.
private struct ResultSummary: View {
    let result: PackResult

    var body: some View {
        HStack(alignment: .top, spacing: 24) {
            SavingsRing(fraction: result.saved, size: 110)
            VStack(alignment: .leading, spacing: 8) {
                Text("\(Format.bytes(result.inputBytes)) → \(Format.bytes(result.outputBytes))")
                    .font(.title3.weight(.semibold))
                    .monospacedDigit()
                Text("\(Format.count(result.files, "file")) in \(Format.duration(result.seconds)) · \(Format.rate(bytes: result.inputBytes, seconds: result.seconds))")
                    .foregroundStyle(.secondary)
                Divider().padding(.vertical, 2)
                ForEach(reasons, id: \.self) { line in
                    Label(line, systemImage: "checkmark.circle.fill")
                        .symbolRenderingMode(.multicolor)
                        .font(.callout)
                }
            }
        }
        .padding(.vertical, 6)
    }

    private var reasons: [String] {
        var lines: [String] = []
        let dup = result.dupFileBytes + result.dupChunkBytes
        if dup > 0 { lines.append("\(Format.bytes(dup)) of duplicates stored once") }
        for t in result.transforms {
            switch t.name {
            case "lepton":
                lines.append("\(Format.count(t.files, "photo")) re-encoded losslessly (\(Format.percent(1 - Double(t.output) / Double(max(t.input, 1)))) smaller)")
            case "deflate":
                lines.append("\(Format.count(t.files, "document or image")) unpacked for stronger compression")
            case "arm64":
                lines.append("\(Format.count(t.files, "app binary")) optimized for Apple silicon code")
            default: break
            }
        }
        lines.append("Every block checked with BLAKE3")
        return lines
    }
}

struct SavingsRing: View {
    let fraction: Double
    var size: CGFloat = 120

    var body: some View {
        ZStack {
            Circle().stroke(.quaternary, lineWidth: size * 0.09)
            Circle()
                .trim(from: 0, to: fraction)
                .stroke(.tint, style: StrokeStyle(lineWidth: size * 0.09, lineCap: .round))
                .rotationEffect(.degrees(-90))
            VStack(spacing: 0) {
                Text(Format.percent(fraction))
                    .font(.system(size: size * 0.24, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text("smaller")
                    .font(.system(size: size * 0.1))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: size, height: size)
    }
}

/// Determinate progress for a running job.
struct JobProgressView: View {
    let job: Job

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(job.title).font(.headline)
            ProgressView(value: job.fraction)
                .progressViewStyle(.linear)
            Text(job.detail)
                .font(.callout)
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
        .padding(.vertical, 24)
    }
}
