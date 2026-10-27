import QuickLook
import SwiftUI

extension FocusedValues {
    /// The archive window in front, for the menu bar's commands.
    @Entry var archiveModel: ArchiveModel?
}

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
        .frame(minWidth: 860, minHeight: 440)
        .task { await model.load() }
        .onDisappear { model.close() }
        .focusedSceneValue(\.archiveModel, model)
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
    @AppStorage("showInspector") private var showInspector = true
    @FocusState private var searchFocused: Bool

    var body: some View {
        NavigationSplitView {
            FolderTree(model: model)
                // A fresh outline for each archive, so nothing of the last one lingers.
                .id(model.levels.map(\.archive.source.path).joined(separator: "\u{1}"))
                .navigationSplitViewColumnWidth(min: 170, ideal: 210, max: 320)
        } detail: {
            table
                .safeAreaInset(edge: .top, spacing: 0) {
                    VStack(spacing: 0) {
                        PathBar(model: model)
                        ForEach(model.changedOutside) { file in ChangedBanner(model: model, file: file) }
                    }
                }
                .safeAreaInset(edge: .bottom, spacing: 0) {
                    if !model.jobs.isEmpty { ActivityBar(model: model) }
                }
                .overlay {
                    if let nested = model.openingNested {
                        ProgressView("Opening “\(nested)”…")
                            .padding(20)
                            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
                    } else if model.visibleItems.isEmpty {
                        if model.search.isEmpty {
                            ContentUnavailableView("Empty Folder", systemImage: "folder")
                        } else if model.searchingInside == nil {
                            ContentUnavailableView.search(text: model.search)
                        }
                    }
                }
                .inspector(isPresented: $showInspector) {
                    InspectorView(model: model)
                        .inspectorColumnWidth(min: 260, ideal: 300, max: 420)
                }
        }
        .searchable(text: $model.search, placement: .toolbar, prompt: "Search archive")
        .searchFocused($searchFocused)
        .modifier(InsideScopes(searchInside: $model.searchInside, shown: hasArchivesInside))
        .onChange(of: model.searchFocusRequest) { searchFocused = true }
        .navigationSubtitle(subtitle)
        .toolbar { toolbar }
        .quickLookPreview($model.quickLookURL, in: model.quickLookURLs)
        .sheet(item: $model.renaming) { item in
            RenameSheet(model: model, item: item)
        }
    }

    private var hasArchivesInside: Bool {
        model.levels.last?.everything.contains(where: \.isArchive) ?? false
    }

    private var subtitle: String {
        guard let l = model.listing else { return "" }
        let kind = l.format.map { "\($0.uppercased()) · " } ?? ""
        return kind + "\(Format.count(l.files, "file")) · \(Format.bytes(l.archiveBytes)) · \(Format.percent(l.saved)) smaller"
    }

    private var table: some View {
        Table(of: Item.self, selection: $model.selection, sortOrder: $model.sortOrder) {
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
                    if !model.search.isEmpty {
                        let location = model.location(of: item)
                        if !location.isEmpty {
                            Text(location)
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                                .truncationMode(.head)
                        }
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
        } rows: {
            ForEach(model.visibleItems) { item in
                TableRow(item)
                    .draggable(ArchiveDrag(item: item, model: model))
            }
        }
        .dropDestination(for: URL.self) { urls, _ in
            guard model.canEdit else { return false }
            model.add(urls)
            return true
        }
        .contextMenu(forSelectionType: Item.ID.self) { ids in
            menu(for: ids)
        } primaryAction: { ids in
            model.activate(ids)
        }
        // Finder's keys: space for Quick Look, typing selects by name.
        .onKeyPress(.space) {
            model.toggleQuickLook()
            return .handled
        }
        .onKeyPress(phases: .down) { press in
            guard press.modifiers.subtracting(.shift).isEmpty else { return .ignored }
            return model.typeSelect(press.characters) ? .handled : .ignored
        }
    }

    @ViewBuilder private func menu(for ids: Set<Item.ID>) -> some View {
        if ids.count == 1, let id = ids.first, let item = model.item(id) {
            if item.inArchive != nil {
                Button("Show in “\((item.inArchive! as NSString).lastPathComponent)”") { model.reveal(item) }
            } else if item.isFolder {
                Button("Open") { model.enter(item) }
            } else if item.isArchive {
                Button("Open Inside Zipling") { model.openNested(item) }
                Button("Open With Default App") { model.openWithDefaultApp(item) }
            } else {
                Button("Open") { model.activate(ids) }
            }
            if !item.isFolder {
                Button("Quick Look") {
                    model.selection = ids
                    model.toggleQuickLook()
                }
                if item.inArchive == nil, !item.isArchive {
                    Button("Edit in Its App") { model.editInApp(item) }
                        .disabled(!model.canEdit)
                }
            }
            Divider()
        }
        if !ids.isEmpty {
            Button("Share…") { model.share(ids) }
        }
        Button(ids.isEmpty ? "Extract All…" : "Extract…") { model.extractInteractively(ids) }
        if ids.isEmpty {
            Button("Extract Here") { model.extractHere() }
        }
        if model.canEdit {
            Divider()
            if ids.count == 1, let id = ids.first, let item = model.item(id), item.inArchive == nil {
                Button("Rename…") { model.renaming = item }
            }
            if !ids.isEmpty {
                Button("Delete from Archive…", role: .destructive) { model.delete(ids) }
            }
            Button("Add Files…") { model.chooseFilesToAdd() }
            Button("New Folder") { model.newFolder() }
        }
    }

    @ToolbarContentBuilder private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .navigation) {
            Button { model.goUp() } label: {
                Label("Back", systemImage: "chevron.backward")
            }
            .disabled(!model.canGoUp)
            .help("Back to the enclosing folder or archive")
        }
        ToolbarItemGroup(placement: .primaryAction) {
            Button { model.chooseFilesToAdd() } label: {
                Label("Add", systemImage: "plus")
            }
            .disabled(!model.canEdit)
            .help(model.editProblem ?? "Add files and folders to the archive")
            Menu {
                Button("Extract Here") { model.extractHere() }
                Button("Extract To…") { model.extractInteractively(model.selection) }
            } label: {
                Label(model.selection.isEmpty ? "Extract All" : "Extract", systemImage: "arrow.down.doc")
            } primaryAction: {
                if model.selection.isEmpty { model.extractHere() } else { model.extractInteractively(model.selection) }
            }
            .help(model.selection.isEmpty ? "Extract everything next to the archive" : "Extract the selected items")
            Button { model.verify() } label: {
                Label("Verify", systemImage: "checkmark.shield")
            }
            .help("Decode everything and check every checksum, writing nothing")
            ShareLink(item: model.url) {
                Label("Share", systemImage: "square.and.arrow.up")
            }
            .help("Share this archive")
            Button { showInspector.toggle() } label: {
                Label("Inspector", systemImage: "sidebar.trailing")
            }
            .keyboardShortcut("i", modifiers: [.command, .option])
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
            if let progress = model.searchingInside, !model.search.isEmpty {
                ProgressView().controlSize(.mini)
                Text("Looking inside \(progress.done + 1) of \(progress.total) archives…")
                    .foregroundStyle(.secondary)
            }
            Text(Format.count(UInt64(model.visibleItems.count), model.search.isEmpty ? "item" : "match"))
                .foregroundStyle(.secondary)
                .monospacedDigit()
        }
        .font(.callout)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(.bar)
        .overlay(alignment: .bottom) { Divider() }
    }

    private func crumb(_ name: String, path: String, icon: String?) -> some View {
        Button {
            model.show(folder: path)
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

/// Extractions and checks running in this window, which keep going while you browse.
private struct ActivityBar: View {
    let model: ArchiveModel

    var body: some View {
        VStack(spacing: 0) {
            Divider()
            ForEach(model.jobs.prefix(3), id: \.id) { job in
                JobRow(job: job) { model.dismiss(job) }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
            }
        }
        .background(.bar)
    }
}

/// One job's progress, then its outcome.
struct JobRow: View {
    let job: Job
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            icon
                .font(.title3)
                .frame(width: 22)
            VStack(alignment: .leading, spacing: 3) {
                Text(job.finishedTitle).lineLimit(1)
                switch job.phase {
                case .running:
                    ProgressView(value: job.fraction).progressViewStyle(.linear)
                    Text(job.detail).font(.caption).foregroundStyle(.secondary).monospacedDigit()
                case .finished:
                    if let outcome = job.outcome {
                        Text(outcome).font(.caption).foregroundStyle(.secondary).lineLimit(2)
                    }
                case .cancelled:
                    Text("Cancelled. Nothing was left behind.").font(.caption).foregroundStyle(.secondary)
                case .failed(let message):
                    Text(message).font(.caption).foregroundStyle(.red).lineLimit(3).textSelection(.enabled)
                }
            }
            Spacer(minLength: 8)
            if job.isRunning {
                Button("Stop", systemImage: "xmark.circle.fill") { job.cancel() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help("Stop")
            } else {
                if !job.placed.isEmpty {
                    Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting(job.placed) }
                        .controlSize(.small)
                }
                Button("Dismiss", systemImage: "xmark") { dismiss() }
                    .labelStyle(.iconOnly)
                    .buttonStyle(.borderless)
                    .help("Dismiss")
            }
        }
    }

    @ViewBuilder private var icon: some View {
        switch job.phase {
        case .running: Image(systemName: "arrow.down.circle").foregroundStyle(.tint)
        case .finished: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
        case .cancelled: Image(systemName: "xmark.circle").foregroundStyle(.secondary)
        case .failed: Image(systemName: "exclamationmark.octagon.fill").foregroundStyle(.red)
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

/// A file opened from the archive was saved in its app: put it back?
private struct ChangedBanner: View {
    let model: ArchiveModel
    let file: ArchiveModel.EditedFile

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "pencil.circle.fill").foregroundStyle(.tint)
            Text("“\(file.name)” was changed in another app.")
            Spacer()
            Button("Ignore") { model.ignoreChanges(file) }
            Button("Update in Archive") { model.saveBack(file) }
                .buttonStyle(.borderedProminent)
        }
        .controlSize(.small)
        .padding(.horizontal, 12)
        .padding(.vertical, 6)
        .background(.tint.opacity(0.08))
        .overlay(alignment: .bottom) { Divider() }
    }
}

private struct RenameSheet: View {
    let model: ArchiveModel
    let item: Item
    @State private var name = ""
    @State private var problem: String?
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Rename “\(item.name)”").font(.headline)
            TextField("Name", text: $name)
                .textFieldStyle(.roundedBorder)
                .onSubmit(submit)
            if let problem {
                Text(problem).font(.callout).foregroundStyle(.red)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Rename", action: submit)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(20)
        .frame(width: 360)
        .onAppear { name = item.name }
    }

    private func submit() {
        problem = model.rename(item, to: name)
        if problem == nil { dismiss() }
    }
}

/// "This Archive / Archives Inside Too" under the search field, only when there are
/// archives inside to search (an empty scope bar would still take up room).
private struct InsideScopes: ViewModifier {
    @Binding var searchInside: Bool
    let shown: Bool

    func body(content: Content) -> some View {
        if shown {
            content.searchScopes($searchInside, activation: .onSearchPresentation) {
                Text("This Archive").tag(false)
                Text("Archives Inside Too").tag(true)
            }
        } else {
            content
        }
    }
}
