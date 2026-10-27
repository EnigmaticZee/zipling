import Quartz
import SwiftUI

/// A live Quick Look preview of a file on disk: the inspector's preview pane.
struct QuickLookPreview: NSViewRepresentable {
    let url: URL

    func makeNSView(context: Context) -> QLPreviewView {
        let view = QLPreviewView(frame: .zero, style: .compact)!
        view.shouldCloseWithWindow = false
        view.autostarts = true
        view.previewItem = url as NSURL
        return view
    }

    func updateNSView(_ view: QLPreviewView, context: Context) {
        if (view.previewItem as? NSURL) as URL? != url { view.previewItem = url as NSURL }
    }

    static func dismantleNSView(_ view: QLPreviewView, coordinator: ()) {
        view.close()
    }
}

/// The selected file's preview, pulled out of the archive when it's small enough
/// to do without asking.
struct PreviewPane: View {
    let model: ArchiveModel
    let item: Item
    @State private var url: URL?
    @State private var failed: String?
    @State private var asked = false

    var body: some View {
        Group {
            if let url {
                QuickLookPreview(url: url)
                    .frame(height: 260)
                    .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.separator))
            } else if let failed {
                Label(failed, systemImage: "eye.slash")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            } else if item.size > ArchiveModel.autoPreviewLimit && !asked {
                Button {
                    asked = true
                } label: {
                    Label("Preview \(Format.bytes(item.size))", systemImage: "eye")
                }
            } else {
                ProgressView()
                    .controlSize(.small)
                    .frame(maxWidth: .infinity, minHeight: 80)
            }
        }
        .task(id: "\(item.id)|\(asked)") {
            url = nil
            failed = nil
            guard item.size <= ArchiveModel.autoPreviewLimit || asked else { return }
            // Let the selection settle while people arrow through a list.
            try? await Task.sleep(for: .milliseconds(140))
            guard !Task.isCancelled else { return }
            do {
                url = try await model.materialize(item)
            } catch is CancellationError {
            } catch {
                failed = "No preview: \(error.localizedDescription)"
            }
        }
    }
}
