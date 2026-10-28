import AppKit
import SwiftUI

/// What can be done with the archive itself: right-click its name at the top of the
/// sidebar or in the path bar.
struct ArchiveMenu: View {
    let model: ArchiveModel

    var body: some View {
        // An archive opened inside another has no file of its own: what acts on a
        // file is offered for the outermost one, named, or left out.
        let nested = model.levels.count > 1
        Button(nested ? "Show “\(model.url.lastPathComponent)” in Finder" : "Show in Finder") {
            model.showInFinder()
        }
        if !nested {
            Button("Copy Path") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(model.url.path, forType: .string)
            }
            OpenWithMenu(url: model.url)
        }
        Divider()
        Button("Extract All…") { model.extractInteractively([]) }
        Button("Extract Here") { model.extractHere() }
        Divider()
        Button("Verify") { model.verify() }
        if !nested {
            Button("Convert…") { model.converting = true }
                .disabled(!model.canConvert)
        }
        if nested {
            // Recovery files belong to files on disk.
        } else if model.hasRecovery {
            Button("Check and Repair") { model.checkAndRepair() }
        } else if model.canAddRecovery {
            Menu("Add Recovery File") {
                ForEach([0.05, 0.10, 0.20], id: \.self) { redundancy in
                    Button("\(Int(redundancy * 100))% of its size") { model.addRecovery(redundancy: redundancy) }
                }
            }
        }
    }
}

/// Open With: the other apps that can open a file, its default app first, as in Finder.
struct OpenWithMenu: View {
    let url: URL

    var body: some View {
        Menu("Open With") {
            let apps = self.apps
            if apps.isEmpty {
                Text("No Other Apps")
            }
            ForEach(apps, id: \.self) { app in
                Button {
                    NSWorkspace.shared.open([url], withApplicationAt: app, configuration: NSWorkspace.OpenConfiguration())
                } label: {
                    Label {
                        Text(FileManager.default.displayName(atPath: app.path).replacingOccurrences(of: ".app", with: ""))
                    } icon: {
                        Image(nsImage: Self.icon(app))
                    }
                }
            }
        }
    }

    /// Any copy of Zipling is left out: the archive is already open in it.
    private var apps: [URL] {
        let workspace = NSWorkspace.shared
        var seen = Set<URL>()
        let preferred = workspace.urlForApplication(toOpen: url).map { [$0] } ?? []
        return (preferred + workspace.urlsForApplications(toOpen: url))
            .map(\.standardizedFileURL)
            .filter { Bundle(url: $0)?.bundleIdentifier != Bundle.main.bundleIdentifier && seen.insert($0).inserted }
    }

    private static func icon(_ app: URL) -> NSImage {
        let image = NSWorkspace.shared.icon(forFile: app.path)
        image.size = NSSize(width: 16, height: 16)
        return image
    }
}
