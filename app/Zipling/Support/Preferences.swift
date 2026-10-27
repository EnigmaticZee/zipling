import Foundation

/// The app's settings, kept in user defaults and edited in Settings (⌘,).
enum Preferences {
    enum Key {
        static let conflict = "conflictPolicy"
        static let trashAfterExtract = "trashAfterExtract"
        static let revealAfterExtract = "revealAfterExtract"
        static let showMenuBarIcon = "showMenuBarIcon"
    }

    /// What to do when an extracted item's name is already taken.
    enum Conflict: String, CaseIterable, Identifiable, Sendable {
        case keepBoth, ask, replace, skip
        var id: String { rawValue }

        var title: String {
            switch self {
            case .ask: "Ask Each Time"
            case .keepBoth: "Keep Both"
            case .replace: "Replace"
            case .skip: "Skip"
            }
        }
    }

    static var conflict: Conflict {
        UserDefaults.standard.string(forKey: Key.conflict).flatMap(Conflict.init) ?? .keepBoth
    }

    /// Move an archive to the Trash once everything in it came out intact.
    static var trashAfterExtract: Bool { UserDefaults.standard.bool(forKey: Key.trashAfterExtract) }

    static var showMenuBarIcon: Bool {
        UserDefaults.standard.object(forKey: Key.showMenuBarIcon) as? Bool ?? true
    }

    static var revealAfterExtract: Bool {
        UserDefaults.standard.object(forKey: Key.revealAfterExtract) as? Bool ?? true
    }
}
