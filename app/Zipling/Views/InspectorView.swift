import SwiftUI

/// Right-hand panel: the archive at a glance, or details for the selected item.
struct InspectorView: View {
    let model: ArchiveModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let item = model.selectedItem {
                    ItemDetails(item: item)
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

        Facts(rows: [
            ("Files", listing.files.formatted()),
            ("Folders", listing.dirs.formatted()),
            ("Blocks", listing.blocks.formatted()),
            ("Created", listing.createdDate.formatted(date: .abbreviated, time: .shortened)),
        ])

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
    let item: Item

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Image(nsImage: FileIcons.icon(forName: item.name, isFolder: item.isFolder))
                .resizable()
                .frame(width: 64, height: 64)
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
            ("Packed", "≈ " + Format.bytes(item.packed)),
            ("Saved", Format.percent(item.saved)),
        ] + (item.isFolder
            ? [("Items", item.childCount.formatted())]
            : [("Method", Method.label(transform: item.transform, codec: item.codec))])
          + [("Modified", item.modified.formatted(date: .abbreviated, time: .shortened))])

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
