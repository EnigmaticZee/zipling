import SwiftUI

/// Sidebar: the folders of the archive on screen as an outline, like Finder's
/// column of places, with the archives inside each one. Selecting a folder shows it
/// in the list; selecting an archive opens it in place.
struct FolderTree: View {
    @Bindable var model: ArchiveModel

    var body: some View {
        List(selection: Binding(get: { model.search.isEmpty ? model.folder : nil },
                                set: { if let path = $0 { model.sidebarSelect(path) } })) {
            if model.levels.count > 1 {
                Section("Opened From") {
                    ForEach(Array(model.levelNames.enumerated().dropLast()), id: \.offset) { index, name in
                        Button { model.leaveTo(level: index) } label: {
                            Label(name, systemImage: "shippingbox")
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            Section("Contents") {
                FolderRow(model: model, path: "", name: model.levelNames.last ?? model.name)
            }
        }
        .listStyle(.sidebar)
        // Finder's key: space previews the selection in the list, wherever focus is.
        .onKeyPress(.space) {
            model.toggleQuickLook()
            return .handled
        }
    }
}

private struct FolderRow: View {
    let model: ArchiveModel
    let path: String
    let name: String

    var body: some View {
        let subfolders = model.subfolders(of: path)
        let archives = model.archives(in: path)
        if subfolders.isEmpty && archives.isEmpty {
            label
        } else {
            DisclosureGroup(isExpanded: Binding(
                get: { model.expanded.contains(path) },
                set: { open in if open { model.expanded.insert(path) } else { model.expanded.remove(path) } })) {
                ForEach(subfolders, id: \.self) { sub in
                    FolderRow(model: model, path: sub, name: (sub as NSString).lastPathComponent)
                }
                ForEach(archives) { item in
                    Label(item.name, systemImage: "shippingbox")
                        .lineLimit(1)
                        .tag(item.id)
                        .help("Open “\(item.name)” inside Zipling")
                        .contextMenu {
                            Button("Open Inside Zipling") { model.openNested(item) }
                            Button("Extract “\(item.name)”…") { model.extractInteractively([item.id]) }
                        }
                }
            } label: {
                label
            }
        }
    }

    @ViewBuilder private var label: some View {
        let row = Label(name, systemImage: path.isEmpty ? "shippingbox" : "folder")
            .lineLimit(1)
            .tag(path)
        if path.isEmpty {
            row.contextMenu { ArchiveMenu(model: model) }
        } else {
            row.contextMenu {
                Button("Extract “\(name)”…") { model.extractInteractively([path]) }
            }
        }
    }
}
