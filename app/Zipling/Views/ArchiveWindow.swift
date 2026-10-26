import SwiftUI

/// A window showing one archive.
struct ArchiveWindow: View {
    let url: URL
    @State private var model: ArchiveModel

    init(url: URL) {
        self.url = url
        _model = State(initialValue: ArchiveModel(url: url))
    }

    var body: some View {
        Group {
            switch model.state {
            case .loading:
                ProgressView("Opening “\(model.name)”…")
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .failed(let message):
                ContentUnavailableView {
                    Label("Can't open “\(model.name)”", systemImage: "exclamationmark.triangle")
                } description: {
                    Text(message)
                }
            case .ready:
                BrowserView(model: model)
            }
        }
        .navigationTitle(model.name)
        .frame(minWidth: 760, minHeight: 420)
        .task { await model.load() }
        .onDisappear { model.close() }
        .sheet(item: $model.passwordRequest) { request in
            PasswordSheet(request: request) { model.passwordRequest = nil }
        }
        .alert(model.alert ?? "", isPresented: Binding(get: { model.alert != nil }, set: { if !$0 { model.alert = nil } })) {
            Button("OK") { model.alert = nil }
        }
    }
}

struct BrowserView: View {
    @Bindable var model: ArchiveModel
    @State private var showInspector = true

    var body: some View {
        table
            .safeAreaInset(edge: .top, spacing: 0) { PathBar(model: model) }
            .overlay {
                if let nested = model.openingNested {
                    ProgressView("Opening “\(nested)”…")
                        .padding(20)
                        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                } else if model.visibleItems.isEmpty {
                    if model.search.isEmpty {
                        ContentUnavailableView("Empty Folder", systemImage: "folder")
                    } else {
                        ContentUnavailableView.search(text: model.search)
                    }
                }
            }
            .searchable(text: $model.search, placement: .toolbar, prompt: "Search archive")
            .navigationSubtitle(subtitle)
            .toolbar { toolbar }
            .inspector(isPresented: $showInspector) {
                InspectorView(model: model)
                    .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
            }
            .sheet(isPresented: Binding(get: { model.job != nil }, set: { if !$0 { model.dismissJob() } })) {
                if let job = model.job {
                    JobSheet(job: job, outcome: model.jobOutcome) { model.dismissJob() }
                }
            }
    }

    private var subtitle: String {
        guard let l = model.listing else { return "" }
        let kind = l.format.map { "\($0.uppercased()) · " } ?? ""
        return kind + "\(Format.count(l.files, "file")) · \(Format.bytes(l.archiveBytes)) · \(Format.percent(l.saved)) smaller"
    }

    private var table: some View {
        Table(model.visibleItems, selection: $model.selection, sortOrder: $model.sortOrder) {
            TableColumn("Name", value: \.name, comparator: .localizedStandard) { item in
                HStack(spacing: 7) {
                    Image(nsImage: FileIcons.icon(forName: item.name, isFolder: item.isFolder))
                        .resizable()
                        .frame(width: 18, height: 18)
                    Text(item.name).lineLimit(1).truncationMode(.middle)
                    if item.encrypted {
                        Image(systemName: "lock.fill")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .help("Encrypted")
                    }
                    if !model.search.isEmpty, !item.parent.isEmpty {
                        Text(item.parent)
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                            .lineLimit(1)
                            .truncationMode(.head)
                    }
                }
            }
            .width(min: 180, ideal: 260)

            TableColumn("Size", value: \.size) { item in
                Text(Format.bytes(item.size))
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .width(min: 70, ideal: 86)

            TableColumn("Saved", value: \.packed) { item in
                if item.packedKnown {
                    SavedBar(fraction: item.saved)
                        .help("≈ \(Format.bytes(item.packed)) packed")
                } else {
                    Text("—")
                        .foregroundStyle(.tertiary)
                        .help("This archive format doesn't record each file's packed size")
                }
            }
            .width(min: 70, ideal: 90)

            TableColumn("Method") { item in
                if !item.isFolder {
                    Text(Method.label(transform: item.transform, codec: item.codec))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .width(min: 124, ideal: 130)

            TableColumn("Modified", value: \.modified) { item in
                Text(item.modified.formatted(.dateTime.day().month(.abbreviated).year().hour().minute()))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            .width(min: 168, ideal: 176)
        }
        .contextMenu(forSelectionType: Item.ID.self) { ids in
            Button(ids.isEmpty ? "Extract All…" : "Extract…") { model.extractInteractively(ids) }
            if ids.count == 1, let id = ids.first, let item = model.item(id), !item.isFolder {
                if item.isArchive {
                    Button("Open Inside Zipling") { model.openNested(item) }
                    Button("Open With Default App") { model.openWithDefaultApp(item) }
                } else {
                    Button("Open") { model.activate(ids) }
                }
            }
        } primaryAction: { ids in
            model.activate(ids)
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            Button { model.goUp() } label: {
                Label("Back", systemImage: "chevron.backward")
            }
            .disabled(!model.canGoUp)
            .keyboardShortcut(.upArrow, modifiers: .command)
        }
        ToolbarItemGroup(placement: .primaryAction) {
            Button { model.extractInteractively(model.selection) } label: {
                Label(model.selection.isEmpty ? "Extract All" : "Extract", systemImage: "arrow.down.doc")
            }
            .help(model.selection.isEmpty ? "Extract everything" : "Extract the selected items")
            Button { model.verify() } label: {
                Label("Verify", systemImage: "checkmark.shield")
            }
            .help("Decode everything and check every checksum, writing nothing")
            Button { showInspector.toggle() } label: {
                Label("Inspector", systemImage: "sidebar.trailing")
            }
        }
    }
}

/// Breadcrumbs: archive › folder › subfolder.
private struct PathBar: View {
    let model: ArchiveModel

    var body: some View {
        HStack(spacing: 4) {
            // Archives opened one inside another: Backup.dmg › photos.7z › …
            ForEach(Array(model.levelNames.enumerated().dropLast()), id: \.offset) { index, name in
                Button { model.leaveTo(level: index) } label: {
                    Label(name, systemImage: "shippingbox")
                }
                .buttonStyle(.plain)
                .lineLimit(1)
                Image(systemName: "chevron.compact.right").foregroundStyle(.tertiary)
            }
            crumb(model.levelNames.last ?? model.name, path: "", icon: "shippingbox")
            ForEach(model.crumbs, id: \.path) { c in
                Image(systemName: "chevron.compact.right").foregroundStyle(.tertiary)
                crumb(c.name, path: c.path, icon: nil)
            }
            Spacer()
            Text(Format.count(UInt64(model.visibleItems.count), "item"))
                .foregroundStyle(.secondary)
        }
        .font(.callout)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(.bar)
        .overlay(alignment: .bottom) { Divider() }
    }

    private func crumb(_ name: String, path: String, icon: String?) -> some View {
        Button {
            model.search = ""
            model.folder = path
            model.selection = []
        } label: {
            if let icon {
                Label(name, systemImage: icon)
            } else {
                Text(name)
            }
        }
        .buttonStyle(.plain)
        .fontWeight(path == model.folder && model.search.isEmpty ? .semibold : .regular)
        .lineLimit(1)
    }
}

/// A small horizontal meter for how much a file shrank.
private struct SavedBar: View {
    let fraction: Double

    var body: some View {
        HStack(spacing: 6) {
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule().fill(.quaternary)
                    Capsule().fill(.tint).frame(width: max(2, geo.size.width * fraction))
                }
            }
            .frame(height: 5)
            Text(Format.percent(fraction))
                .font(.caption)
                .monospacedDigit()
                .foregroundStyle(.secondary)
                .frame(width: 32, alignment: .trailing)
        }
    }
}

/// Progress, then the outcome, of an extraction or verification.
struct JobSheet: View {
    let job: Job
    let outcome: String?
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            switch job.phase {
            case .running:
                JobProgressView(job: job)
                HStack {
                    Spacer()
                    Button("Cancel", role: .cancel) { job.cancel() }
                        .keyboardShortcut(.cancelAction)
                }
            case .finished:
                Label("Done", systemImage: "checkmark.circle.fill")
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.green)
                if let outcome { Text(outcome).foregroundStyle(.secondary) }
                closeButton
            case .cancelled:
                Label("Cancelled", systemImage: "xmark.circle")
                    .font(.title3.weight(.semibold))
                Text("Nothing was left behind.").foregroundStyle(.secondary)
                closeButton
            case .failed(let message):
                Label("Something went wrong", systemImage: "exclamationmark.octagon.fill")
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.red)
                Text(message).textSelection(.enabled)
                closeButton
            }
        }
        .padding(24)
        .frame(width: 440)
    }

    private var closeButton: some View {
        HStack {
            Spacer()
            Button("OK", action: dismiss).keyboardShortcut(.defaultAction)
        }
    }
}

/// Asks for an archive's password; a wrong one keeps the sheet up with a message.
private struct PasswordSheet: View {
    let request: PasswordRequest
    let dismiss: () -> Void
    @State private var password = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Label("“\(request.archiveName)” is encrypted", systemImage: "lock.fill")
                .font(.title3.weight(.semibold))
            Text(request.wrong ? "That password didn't work. Try again." : "Enter its password to continue.")
                .foregroundStyle(request.wrong ? .red : .secondary)
            SecureField("Password", text: $password)
                .textFieldStyle(.roundedBorder)
                .onSubmit(submit)
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) {
                    dismiss()
                    request.giveUp()
                }
                .keyboardShortcut(.cancelAction)
                Button("Unlock", action: submit)
                    .keyboardShortcut(.defaultAction)
                    .disabled(password.isEmpty)
            }
        }
        .padding(24)
        .frame(width: 400)
    }

    private func submit() {
        guard !password.isEmpty else { return }
        dismiss()
        request.retry(password)
    }
}
