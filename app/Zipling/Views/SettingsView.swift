import SwiftUI

/// Settings (⌘,): how extraction behaves.
struct SettingsView: View {
    @AppStorage(Preferences.Key.conflict) private var conflict: Preferences.Conflict = .keepBoth
    @AppStorage(Preferences.Key.trashAfterExtract) private var trashAfterExtract = false
    @AppStorage(Preferences.Key.revealAfterExtract) private var revealAfterExtract = true
    @AppStorage(Preferences.Key.showMenuBarIcon) private var showMenuBarIcon = true
    @State private var shrink = ShrinkLater.shared
    @AppStorage(Preferences.Key.indexArchives) private var indexArchives = true
    @AppStorage(Preferences.Key.checkNewArchives) private var checkNewArchives = false
    @State private var automationNote: String?

    var body: some View {
        Form {
            Section("Compressing") {
                Toggle(isOn: $checkNewArchives) {
                    Text("Check new ZIP, 7z and TAR archives")
                    Text("Reads each one back in full before it's put in place, which can double the time for a ZIP. Zipling archives are checked by the engine as they're made, and converting or editing an archive always checks.")
                }
            }
            Section("Extracting") {
                Picker(selection: $conflict) {
                    ForEach(Preferences.Conflict.allCases) { Text($0.title).tag($0) }
                } label: {
                    Text("When a name is already taken")
                    Text("Keep Both gives the new one a numbered name, as Finder does. Replace moves the old one to the Trash.")
                }
                Toggle("Show extracted items in Finder", isOn: $revealAfterExtract)
                Toggle(isOn: $trashAfterExtract) {
                    Text("Move archives to the Trash after extracting")
                    Text("Only when everything came out and passed the archive's checksums. Split archives go with all their parts.")
                }
            }
            Section("Shrink Later") {
                LabeledContent {
                    Button("Shrink Now") { Task { await shrink.shrinkAll() } }
                        .disabled(shrink.queue.isEmpty || shrink.running)
                } label: {
                    Text(shrink.queue.isEmpty ? "Nothing waiting"
                         : shrink.queue.count == 1 ? "1 archive waiting" : "\(shrink.queue.count) archives waiting")
                    Text("Archives made at Fast or Balanced with “Shrink it to Max later” are re-packed while the Mac is idle and plugged in. The files inside stay the same.")
                }
            }
            Section("Menu Bar") {
                Toggle(isOn: $showMenuBarIcon) {
                    Text("Show Zipling in the menu bar")
                    Text("Drop files on it to compress them, or archives to extract them.")
                }
                .onChange(of: showMenuBarIcon) { MenuBarDropZone.shared.update() }
            }
            Section("Spotlight") {
                Toggle(isOn: $indexArchives) {
                    Text("Let Spotlight find files inside archives you open")
                    Text("Searching for a file's name finds the archive it's in. The index stays on this Mac.")
                }
                Button("Remove Zipling's Entries from Spotlight") { Task { await Spotlight.removeAll() } }
            }
            Section("Automation") {
                Text("Shortcuts has Compress Files, Extract Archive and Convert Archive actions. AppleScript can tell Zipling to compress, extract and convert.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                LabeledContent {
                    Button("Install") {
                        do {
                            let folder = try Automation.installFolderActions()
                            automationNote = "Installed. Right-click a folder › Services › Folder Actions Setup, and attach a Zipling script."
                            NSWorkspace.shared.activateFileViewerSelecting([folder])
                        } catch {
                            automationNote = error.localizedDescription
                        }
                    }
                } label: {
                    Text("Folder Actions")
                    Text(automationNote ?? "Scripts that extract archives, or compress items, as they're added to a folder.")
                }
            }
            Section("Finder") {
                Text("Right-click files in Finder and choose Quick Actions or Services: Compress with Zipling, Extract Here with Zipling, Open in Zipling. Hold Option when dropping archives on Zipling's Dock icon to extract them right away.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .formStyle(.grouped)
        .frame(width: 480)
        .fixedSize()
    }
}
