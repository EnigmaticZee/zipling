import SwiftUI

/// Sidebar: the folders of the archive on screen as an outline, like Finder's
/// column of places. Selecting one shows it in the list.
struct FolderTree: View {
    @Bindable var model: ArchiveModel

    var body: some View {
        List(selection: Binding(get: { model.search.isEmpty ? model.folder : nil },
                                set: { if let path = $0 { model.show(folder: path) } })) {
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
    }
}

private struct FolderRow: View {
    let model: ArchiveModel
    let path: String
    let name: String

    var body: some View {
        let subfolders = model.subfolders(of: path)
        if subfolders.isEmpty {
            label
        } else {
            DisclosureGroup(isExpanded: Binding(
                get: { model.expanded.contains(path) },
                set: { open in if open { model.expanded.insert(path) } else { model.expanded.remove(path) } })) {
                ForEach(subfolders, id: \.self) { sub in
                    FolderRow(model: model, path: sub, name: (sub as NSString).lastPathComponent)
                }
            } label: {
                label
            }
        }
    }

    private var label: some View {
        Label(name, systemImage: path.isEmpty ? "shippingbox" : "folder")
            .lineLimit(1)
            .tag(path)
    }
}
