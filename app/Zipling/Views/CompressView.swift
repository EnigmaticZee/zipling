import SwiftUI
import UniformTypeIdentifiers

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
    @AppStorage("createOptions") private var stored = CreateOptions().encoded
    @State private var options = CreateOptions()
    @State private var output: URL
    @State private var contents: ContentSummary?
    @State private var usePassword = false
    @State private var password = ""
    @State private var confirmation = ""
    @State private var presets = Presets.saved()
    @State private var savingPreset = false
    @State private var presetName = ""
    @State private var job: Job?
    @State private var made: Creation.Made?

    init(items: [URL]) {
        self.items = items
        _output = State(initialValue: Self.defaultOutput(for: items, format: .zlg))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            header
            if let job, let made, case .finished = job.phase {
                ResultSummary(result: made.result, format: options.format, parts: made.files.count)
            } else if let job {
                JobProgressView(job: job)
            } else {
                formatPicker
                if let advice, advice.options.format != options.format || advice.options.level != options.level {
                    AdviceBanner(advice: advice) { apply(advice.options) }
                }
                if options.format == .zlg {
                    levelPicker
                } else {
                    settings
                }
                destination
            }
            footer
        }
        .padding(24)
        .frame(width: 640)
        .task {
            if let saved = CreateOptions(encoded: stored) { apply(saved) }
            contents = await ContentSummary.measure(items)
        }
        .sheet(isPresented: $savingPreset) { savePresetSheet }
    }

    private var advice: Advice? { contents.map(Advice.init) }

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
            Spacer()
            presetMenu
        }
    }

    private var title: String {
        items.count == 1 ? "Compress “\(items[0].lastPathComponent)”" : "Compress \(items.count) items"
    }

    private var subtitle: String {
        guard let contents else { return "Measuring…" }
        return "\(Format.count(contents.files, "file")) · \(Format.bytes(contents.bytes))"
    }

    private var formatPicker: some View {
        VStack(alignment: .leading, spacing: 6) {
            Picker("Format", selection: Binding(get: { options.format }, set: { setFormat($0) })) {
                ForEach(ArchiveFormat.allCases) { Text($0.title).tag($0) }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            Text(options.format.blurb)
                .font(.callout)
                .foregroundStyle(.secondary)
        }
    }

    private var levelPicker: some View {
        HStack(spacing: 10) {
            ForEach(Level.allCases) { option in
                LevelCard(level: option, selected: option == options.level, recommended: option == .balanced) {
                    options.level = option
                }
            }
        }
    }

    private var settings: some View {
        Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 14, verticalSpacing: 12) {
            if options.hasLevels {
                GridRow {
                    Text("Compression").foregroundStyle(.secondary).gridColumnAlignment(.trailing)
                    Picker("Compression", selection: $options.level) {
                        ForEach(Level.allCases) { Text($0.title).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                    .frame(width: 320)
                }
            }
            if options.format.supportsPassword {
                GridRow {
                    Text("Password").foregroundStyle(.secondary)
                    VStack(alignment: .leading, spacing: 6) {
                        Toggle("Encrypt with AES-256", isOn: $usePassword)
                        if usePassword {
                            HStack {
                                SecureField("Password", text: $password)
                                SecureField("Confirm", text: $confirmation)
                            }
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 320)
                            if options.format == .sevenZip {
                                Toggle("Hide file names too", isOn: $options.encryptNames)
                            } else {
                                Text("Opens in 7-Zip, WinRAR, Keka and Zipling. Windows' built-in ZIP support can't open encrypted ZIPs, and their file names stay visible.")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }
            if options.format.supportsSplit {
                GridRow {
                    Text("Split").foregroundStyle(.secondary)
                    Picker("Split", selection: $options.splitBytes) {
                        ForEach(SplitSize.all, id: \.bytes) { Text($0.title).tag($0.bytes) }
                    }
                    .labelsHidden()
                    .frame(width: 240)
                }
            }
            GridRow {
                Text("Mac files").foregroundStyle(.secondary)
                Toggle("Leave out .DS_Store and other Mac-only files", isOn: $options.leaveOutMacFiles)
            }
        }
        .font(.callout)
    }

    private var destination: some View {
        HStack(spacing: 10) {
            Image(nsImage: NSWorkspace.shared.icon(for: UTType(filenameExtension: options.format.fileExtension) ?? .archive))
                .resizable()
                .frame(width: 22, height: 22)
            VStack(alignment: .leading, spacing: 1) {
                Text(output.lastPathComponent + (options.splitBytes != nil && options.format.supportsSplit ? " (.001, .002…)" : ""))
                    .lineLimit(1)
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

    private var presetMenu: some View {
        Menu {
            Section("Presets") {
                ForEach(Presets.builtIn) { preset in
                    Button(preset.name) { apply(preset.options, password: preset.usesPassword) }
                }
            }
            if !presets.isEmpty {
                Section("Yours") {
                    ForEach(presets) { preset in
                        Button(preset.name) { apply(preset.options, password: preset.usesPassword) }
                    }
                }
                Menu("Delete Preset") {
                    ForEach(presets) { preset in
                        Button(preset.name, role: .destructive) {
                            presets.removeAll { $0.id == preset.id }
                            Presets.save(presets)
                        }
                    }
                }
            }
            Divider()
            Button("Save as Preset…") {
                presetName = ""
                savingPreset = true
            }
        } label: {
            Label("Presets", systemImage: "slider.horizontal.3")
        }
        .menuStyle(.button)
        .fixedSize()
        .disabled(job != nil)
    }

    private var savePresetSheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Save as Preset").font(.headline)
            Text("Keeps the format and settings, never the password.")
                .font(.callout)
                .foregroundStyle(.secondary)
            TextField("Name", text: $presetName)
                .textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { savingPreset = false }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    presets.append(CreatePreset(name: presetName, options: options, usesPassword: usePassword))
                    Presets.save(presets)
                    savingPreset = false
                }
                .keyboardShortcut(.defaultAction)
                .disabled(presetName.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 340)
    }

    @ViewBuilder private var footer: some View {
        HStack {
            if let job, let made, case .finished = job.phase {
                Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting(made.files) }
                ShareLink("Share", items: made.files)
                Button("Open Archive") {
                    AppRouter.shared.open([made.files[0]])
                    dismiss()
                }
                Spacer()
                Text("\(Format.bytes(made.result.outputBytes)) in \(Format.duration(made.result.seconds))")
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
                } else if let problem {
                    Text(problem).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button(job == nil ? "Compress" : "Try Again") { start() }
                    .keyboardShortcut(.defaultAction)
                    .buttonStyle(.borderedProminent)
                    .disabled(problem != nil)
            }
        }
        .controlSize(.large)
    }

    /// Why Compress can't start yet.
    private var problem: String? {
        guard usePassword, options.format.supportsPassword else { return nil }
        if password.isEmpty { return "Enter a password." }
        if password != confirmation { return "The passwords don't match." }
        return nil
    }

    // MARK: Actions

    private func setFormat(_ format: ArchiveFormat) {
        options.format = format
        let name = Archives.baseName(output.lastPathComponent)
        output = Placement.unique("\(name).\(format.fileExtension)", in: output.deletingLastPathComponent())
    }

    private func apply(_ new: CreateOptions, password wanted: Bool? = nil) {
        if new.format != options.format { setFormat(new.format) }
        options = new
        if let wanted { usePassword = wanted && new.format.supportsPassword }
    }

    private func start() {
        let job = Job(title: "Compressing as \(options.format.title)…")
        self.job = job
        made = nil
        JobCenter.shared.track(job)
        stored = options.encoded
        let (items, output, options) = (items, output, options)
        let password = usePassword && options.format.supportsPassword ? password : nil
        Task {
            do {
                let result = try await Creation.create(
                    items, into: output, options: options, password: password, task: job.task,
                    cancel: job.cancelToken, progress: job.progressHandler)
                made = result
                job.finish(nil, outcome: "Made \(result.files.map(\.lastPathComponent).joined(separator: ", ")).",
                           placed: result.files)
                if let first = result.files.first { AppRouter.shared.noteCreated(first) }
            } catch {
                job.finish(error)
            }
        }
    }

    private func chooseOutput() {
        let panel = NSSavePanel()
        if let type = UTType(filenameExtension: options.format.fileExtension) { panel.allowedContentTypes = [type] }
        panel.directoryURL = output.deletingLastPathComponent()
        panel.nameFieldStringValue = output.lastPathComponent
        panel.canCreateDirectories = true
        if panel.runModal() == .OK, let url = panel.url { output = url }
    }

    // MARK: Helpers

    private static func defaultOutput(for items: [URL], format: ArchiveFormat) -> URL {
        let folder = items.first?.deletingLastPathComponent() ?? URL.homeDirectory
        let base = items.count == 1 ? items[0].deletingPathExtension().lastPathComponent : "Archive"
        return Placement.unique("\(base).\(format.fileExtension)", in: folder)
    }
}

/// Sizes to split an archive into, named for where the parts are going.
struct SplitSize {
    let title: String
    let bytes: UInt64?

    static let all: [SplitSize] = [
        SplitSize(title: "Don't split", bytes: nil),
        SplitSize(title: "20 MB (email)", bytes: 20_000_000),
        SplitSize(title: "100 MB", bytes: 100_000_000),
        SplitSize(title: "700 MB (CD)", bytes: 700 << 20),
        SplitSize(title: "2 GB", bytes: 2_000_000_000),
        SplitSize(title: "4 GB (FAT32 drives)", bytes: (4 << 30) - 1),
        SplitSize(title: "4.7 GB (DVD)", bytes: 4_690_000_000),
        SplitSize(title: "25 GB (Blu-ray)", bytes: 25_000_000_000),
    ]
}

/// A named set of options. Passwords are never saved, only whether to ask for one.
struct CreatePreset: Codable, Identifiable, Equatable {
    var id = UUID()
    var name: String
    var options: CreateOptions
    var usesPassword = false
}

enum Presets {
    private static let key = "createPresets"
    /// Where they're kept (tests use their own).
    nonisolated(unsafe) static var defaults = UserDefaults.standard

    static let builtIn: [CreatePreset] = [
        CreatePreset(name: "Smallest (Zipling Max)", options: CreateOptions(format: .zlg, level: .max)),
        CreatePreset(name: "Share with anyone (ZIP)", options: CreateOptions(format: .zip, level: .balanced)),
        CreatePreset(name: "Email-sized parts (ZIP, 20 MB)",
                     options: CreateOptions(format: .zip, level: .balanced, splitBytes: 20_000_000)),
        CreatePreset(name: "Private (7z, names hidden)",
                     options: CreateOptions(format: .sevenZip, level: .max, encryptNames: true), usesPassword: true),
        CreatePreset(name: "Linux and servers (TAR.XZ)", options: CreateOptions(format: .tarXz, level: .max)),
    ]

    static func saved() -> [CreatePreset] {
        guard let data = defaults.data(forKey: key) else { return [] }
        return (try? JSONDecoder().decode([CreatePreset].self, from: data)) ?? []
    }

    static func save(_ presets: [CreatePreset]) {
        defaults.set(try? JSONEncoder().encode(presets), forKey: key)
    }
}

extension CreateOptions {
    /// As text for @AppStorage: the last options used come back next time.
    var encoded: String { (try? JSONEncoder().encode(self)).map { String(decoding: $0, as: UTF8.self) } ?? "" }

    init?(encoded: String) {
        guard let value = try? JSONDecoder().decode(CreateOptions.self, from: Data(encoded.utf8)) else { return nil }
        self = value
    }
}

/// What's being compressed, by the kind of data that matters for the choice of format.
struct ContentSummary: Sendable {
    var files: UInt64 = 0
    var bytes: UInt64 = 0
    /// JPEGs: Zipling re-encodes them losslessly.
    var photos: UInt64 = 0
    /// PDFs, PNGs and Office files: Deflate inside, which Zipling unpacks.
    var documents: UInt64 = 0
    /// Video, music, HEIC and archives: already compressed, nothing helps much.
    var media: UInt64 = 0

    func share(_ part: UInt64) -> Double { bytes == 0 ? 0 : Double(part) / Double(bytes) }

    static func measure(_ items: [URL]) async -> ContentSummary {
        await Task.detached(priority: .utility) {
            var summary = ContentSummary()
            let keys: Set<URLResourceKey> = [.fileSizeKey, .isRegularFileKey]
            func add(_ url: URL, _ size: UInt64) {
                summary.files += 1
                summary.bytes += size
                switch url.pathExtension.lowercased() {
                case "jpg", "jpeg": summary.photos += size
                case "pdf", "png", "docx", "xlsx", "pptx", "key", "pages", "numbers", "odt", "ods", "epub": summary.documents += size
                case "mp4", "mov", "m4v", "mkv", "avi", "mp3", "m4a", "aac", "flac", "heic", "heif", "webp", "avif",
                     "gif", "zip", "7z", "rar", "gz", "xz", "bz2", "zst", "dmg", "pkg", "zlg":
                    summary.media += size
                default: break
                }
            }
            for url in items {
                if let v = try? url.resourceValues(forKeys: keys), v.isRegularFile == true {
                    add(url, UInt64(v.fileSize ?? 0))
                    continue
                }
                let walker = FileManager.default.enumerator(at: url, includingPropertiesForKeys: Array(keys))
                while let file = walker?.nextObject() as? URL {
                    if let v = try? file.resourceValues(forKeys: keys), v.isRegularFile == true {
                        add(file, UInt64(v.fileSize ?? 0))
                    }
                }
            }
            return summary
        }.value
    }
}

/// The format and level that suit what's being compressed, and why.
struct Advice {
    let options: CreateOptions
    let reason: String

    init(_ contents: ContentSummary) {
        if contents.share(contents.media) > 0.7 {
            options = CreateOptions(format: .zlg, level: .fast)
            reason = "Mostly video, music or archives, which are already compressed. Fast finishes in seconds; slower settings would barely help."
        } else if contents.share(contents.photos) > 0.4 {
            options = CreateOptions(format: .zlg, level: .balanced)
            reason = "Mostly photos. Zipling re-encodes JPEGs losslessly, about a fifth smaller. ZIP and 7z save almost nothing on them."
        } else if contents.share(contents.documents) > 0.4 {
            options = CreateOptions(format: .zlg, level: .balanced)
            reason = "Mostly PDFs, PNGs or Office files. Zipling unpacks their built-in compression and redoes it far better: often half the size."
        } else {
            options = CreateOptions(format: .zlg, level: .balanced)
            reason = "Balanced is smaller than 7-Zip's best and several times faster. Choose ZIP when sending to someone without Zipling."
        }
    }
}

private struct AdviceBanner: View {
    let advice: Advice
    let use: () -> Void

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: "lightbulb")
                .foregroundStyle(.yellow)
            VStack(alignment: .leading, spacing: 2) {
                Text("Suggested: \(advice.options.format.title) \(advice.options.level.title)")
                    .font(.callout.weight(.semibold))
                Text(advice.reason)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer()
            Button("Use", action: use)
                .controlSize(.small)
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 10, style: .continuous).fill(.yellow.opacity(0.08)))
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
    let format: ArchiveFormat
    let parts: Int

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
        if parts > 1 { lines.append("Split into \(parts) parts") }
        lines.append(format == .zlg ? "Every block checked with BLAKE3" : "Read back and checked after writing")
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
