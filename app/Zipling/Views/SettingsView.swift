import SwiftUI

/// Settings (⌘,): how extraction behaves.
struct SettingsView: View {
    @AppStorage(Preferences.Key.conflict) private var conflict: Preferences.Conflict = .keepBoth
    @AppStorage(Preferences.Key.trashAfterExtract) private var trashAfterExtract = false
    @AppStorage(Preferences.Key.revealAfterExtract) private var revealAfterExtract = true
    @AppStorage(Preferences.Key.showMenuBarIcon) private var showMenuBarIcon = true

    var body: some View {
        Form {
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
            Section("Menu Bar") {
                Toggle(isOn: $showMenuBarIcon) {
                    Text("Show Zipling in the menu bar")
                    Text("Drop files on it to compress them, or archives to extract them.")
                }
                .onChange(of: showMenuBarIcon) { MenuBarDropZone.shared.update() }
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
