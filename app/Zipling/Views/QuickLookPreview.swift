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
            } catch let p as PasswordRequired {
                // A wrong password: forget it, so Unlock to Preview comes back.
                model.forgetPassword()
                failed = p.wrong ? "That password didn't work." : "Locked. Unlock it to preview."
            } catch {
                failed = "No preview: \(error.localizedDescription)"
            }
        }
    }
}

/// Finder's Quick Look: the app drives the panel, so an arrow key moves the list's
/// selection behind it and the preview follows, instead of the panel keeping the key
/// for itself. The app delegate, the last stop of the responder chain the panel
/// searches for a controller, hands the panel to this object.
@MainActor
final class QuickLookPanelController: NSObject, @preconcurrency QLPreviewPanelDataSource, @preconcurrency QLPreviewPanelDelegate {
    static let shared = QuickLookPanelController()
    /// The archive window whose selection the panel shows.
    private(set) weak var model: ArchiveModel?

    private var panel: QLPreviewPanel? { QLPreviewPanel.sharedPreviewPanelExists() ? QLPreviewPanel.shared() : nil }
    private var inControl: Bool { panel?.dataSource === self }

    var isVisible: Bool { panel?.isVisible ?? false }
    func isShowing(_ model: ArchiveModel) -> Bool { self.model === model && isVisible }

    /// Shows the panel for `model`'s selection, or brings it up to date if it is up.
    func show(for model: ArchiveModel) {
        if let old = self.model, old !== model { old.quickLookClosed() }
        self.model = model
        guard let p = QLPreviewPanel.shared() else { return }
        if p.isVisible {
            if !inControl { p.updateController() }
            p.reloadData()
            land()
        } else {
            p.makeKeyAndOrderFront(nil)
        }
    }

    func hide() {
        panel?.orderOut(nil)
        model?.quickLookClosed()
    }

    /// `model`'s files changed: the panel reloads and lands on the current one.
    func refresh(for model: ArchiveModel) {
        guard self.model === model, isVisible, inControl, let p = panel else { return }
        p.reloadData()
        land()
    }

    private func land() {
        guard let model, let p = panel, let url = model.quickLookURL,
              let i = model.quickLookURLs.firstIndex(of: url), p.currentPreviewItemIndex != i else { return }
        p.currentPreviewItemIndex = i
    }

    // MARK: Control, handed over by the app delegate

    func begin(_ p: QLPreviewPanel) {
        p.dataSource = self
        p.delegate = self
        land()
    }

    func end(_ p: QLPreviewPanel) {
        p.dataSource = nil
        p.delegate = nil
        model?.quickLookClosed()
    }

    // MARK: Data source

    func numberOfPreviewItems(in panel: QLPreviewPanel!) -> Int {
        model?.quickLookURLs.count ?? 0
    }

    func previewPanel(_ panel: QLPreviewPanel!, previewItemAt index: Int) -> (any QLPreviewItem)! {
        guard let urls = model?.quickLookURLs, urls.indices.contains(index) else { return nil }
        return urls[index] as NSURL
    }

    // MARK: Delegate

    /// The keys the panel does not take for itself: the arrows move the selection, as
    /// in Finder, and Space closes the panel again.
    func previewPanel(_ panel: QLPreviewPanel!, handle event: NSEvent!) -> Bool {
        guard event.type == .keyDown, let model,
              event.modifierFlags.intersection([.command, .option, .control]).isEmpty else { return false }
        switch event.keyCode {
        case 125, 124: model.moveQuickLookSelection(by: 1)      // down, right
        case 126, 123: model.moveQuickLookSelection(by: -1)     // up, left
        case 115: model.moveQuickLookSelection(toEnd: false)    // home
        case 119: model.moveQuickLookSelection(toEnd: true)     // end
        case 49, 53: hide()                                     // space, escape
        default: return false
        }
        return true
    }
}
