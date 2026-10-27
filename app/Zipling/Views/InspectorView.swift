import SwiftUI

/// Right-hand panel: the archive at a glance, or details for the selected item.
struct InspectorView: View {
    let model: ArchiveModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let item = model.selectedItem {
                    ItemDetails(model: model, item: item)
                } else if let listing = model.listing {
                    ArchiveSummary(model: model, listing: listing)
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct ArchiveSummary: View {
    let model: ArchiveModel
    let listing: Listing

    var body: some View {
        VStack(spacing: 10) {
            SavingsRing(fraction: listing.saved, size: 132)
            Text("\(Format.bytes(listing.totalSize)) → \(Format.bytes(listing.archiveBytes))")
                .font(.headline)
                .monospacedDigit()
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 6)

        if listing.isZipling {
            Facts(rows: [
                ("Files", listing.files.formatted()),
                ("Folders", listing.dirs.formatted()),
                ("Blocks", listing.blocks.formatted()),
                ("Created", listing.createdDate.formatted(date: .abbreviated, time: .shortened)),
            ])
            zipling
        } else {
            Facts(rows: [
                ("Format", (listing.format ?? "Archive").uppercased()),
                ("Method", listing.method.map { Method.foreignName($0) } ?? "—"),
                ("Solid", listing.solid == true ? "Yes" : "No"),
                ("Encrypted", listing.encrypted == true ? "Yes" : "No"),
                ("Files", listing.files.formatted()),
                ("Folders", listing.dirs.formatted()),
                ("Modified", listing.createdDate.formatted(date: .abbreviated, time: .shortened)),
            ])
            if model.canChangeEncoding {
                InspectorSection("File names") {
                    Picker("Encoding", selection: Binding(
                        get: { model.chosenEncoding }, set: { model.reopen(encoding: $0) })) {
                        Text("Automatic").tag(String?.none)
                        Divider()
                        ForEach(Unar.encodings, id: \.0) { name, label in
                            Text(label).tag(Optional(name))
                        }
                    }
                    .labelsHidden()
                    Text(encodingNote)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
            InspectorSection("Integrity") {
                Text("Decode everything and check the archive's own checksums, without writing anything.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Button {
                    model.verify()
                } label: {
                    Label("Verify Now", systemImage: "checkmark.shield")
                }
            }
            ArchiveTools(model: model)
        }
    }

    private var encodingNote: String {
        if let detected = model.detectedEncoding {
            return "Detected \(Unar.label(detected)). If names look wrong, pick the language the archive was made in."
        }
        if model.chosenEncoding != nil { return "Names are read with the encoding you picked." }
        return "Names are shown as stored. If they look wrong, pick the language the archive was made in."
    }

    @ViewBuilder private var zipling: some View {
        InspectorSection("How it got smaller") {
            ForEach(breakdown, id: \.title) { row in
                HStack(alignment: .firstTextBaseline, spacing: 10) {
                    Image(systemName: row.symbol)
                        .foregroundStyle(.tint)
                        .frame(width: 18)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(row.title)
                        Text("\(Format.count(row.files, "file")) · \(Format.bytes(row.size)) → ≈ \(Format.bytes(row.packed))")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }

        InspectorSection("Integrity") {
            Text("Every block carries BLAKE3 hashes and is checked during extraction.")
                .font(.callout)
                .foregroundStyle(.secondary)
            Button {
                model.verify()
            } label: {
                Label("Verify Now", systemImage: "checkmark.shield")
            }
        }
        ArchiveTools(model: model)
    }

    private struct Row {
        let title: String
        let symbol: String
        var files: UInt64 = 0
        var size: UInt64 = 0
        var packed: UInt64 = 0
    }

    private var breakdown: [Row] {
        var rows: [String: Row] = [:]
        for e in listing.entries where e.kind == "file" {
            let (key, title) = switch e.transform {
            case "lepton": ("lepton", "Photos re-encoded (Lepton)")
            case "deflate": ("deflate", "Documents & images unpacked")
            case "arm64": ("arm64", "App code optimized")
            default: (e.codec, e.codec == "store" ? "Stored as-is" : "Compressed with \(Method.codecName(e.codec))")
            }
            rows[key, default: Row(title: title, symbol: Method.symbol(transform: e.transform))].files += 1
            rows[key]!.size += e.size
            rows[key]!.packed += e.packed
        }
        return rows.values.sorted { $0.size > $1.size }
    }
}

private struct ItemDetails: View {
    let model: ArchiveModel
    let item: Item
    private var locked: Bool { item.encrypted && !model.hasPassword }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            if item.isFolder || locked {
                Image(nsImage: FileIcons.icon(forName: item.name, isFolder: item.isFolder))
                    .resizable()
                    .frame(width: 64, height: 64)
            } else {
                PreviewPane(model: model, item: item)
            }
            Text(item.name)
                .font(.title3.weight(.semibold))
                .textSelection(.enabled)
            if !item.parent.isEmpty {
                Text(item.parent)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
        }

        Facts(rows: [
            ("Size", Format.bytes(item.size)),
            ("Packed", item.packedKnown ? "≈ " + Format.bytes(item.packed) : "—"),
            ("Saved", item.packedKnown ? Format.percent(item.saved) : "—"),
        ] + (item.isFolder
            ? [("Items", item.childCount.formatted())]
            : [("Method", Method.label(transform: item.transform, codec: item.codec))])
          + [("Modified", item.modified.formatted(date: .abbreviated, time: .shortened))])

        if locked && !item.isFolder {
            Button {
                model.unlock {}
            } label: {
                Label("Unlock to Preview", systemImage: "lock.open")
            }
        }

        if !item.isFolder {
            InspectorSection("How it's stored") {
                Text(Method.explanation(transform: item.transform))
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

private struct Facts: View {
    let rows: [(String, String)]

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
            ForEach(rows, id: \.0) { label, value in
                GridRow {
                    Text(label).foregroundStyle(.secondary)
                    Text(value).monospacedDigit().textSelection(.enabled)
                }
            }
        }
        .font(.callout)
    }
}

private struct InspectorSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
            content
        }
    }
}

/// Converting and recovery files, for the window's own archive.
private struct ArchiveTools: View {
    let model: ArchiveModel

    var body: some View {
        if model.canConvert {
            InspectorSection("Convert") {
                Text(model.listing?.isZipling == true
                     ? "Make it a ZIP or 7z for people without Zipling, or re-pack it at another level. The files stay the same."
                     : "Make it a Zipling archive, usually much smaller, or a tighter ZIP or 7z. The files stay the same.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Button {
                    model.converting = true
                } label: {
                    Label("Convert…", systemImage: "arrow.triangle.2.circlepath")
                }
            }
        }
        if model.canAddRecovery {
            InspectorSection("Recovery") {
                if let capacity = model.recoveryCapacity {
                    Text("Its recovery file can repair up to \(Format.bytes(capacity)) of damage, from a failing disk or a broken download.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Button {
                        model.checkAndRepair()
                    } label: {
                        Label("Check and Repair", systemImage: "bandage")
                    }
                } else {
                    Text("Save repair data next to the archive, so damage from a failing disk or a broken download can be fixed later.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    Menu {
                        Button("5% of its size") { model.addRecovery(redundancy: 0.05) }
                        Button("10% of its size") { model.addRecovery(redundancy: 0.10) }
                        Button("20% of its size") { model.addRecovery(redundancy: 0.20) }
                    } label: {
                        Label("Add Recovery File", systemImage: "cross.case")
                    }
                    .fixedSize()
                }
            }
        }
    }
}
