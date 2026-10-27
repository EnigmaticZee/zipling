import CoreSpotlight
import UniformTypeIdentifiers

/// Spotlight finds files inside archives Zipling has opened: each archive is indexed
/// with the names inside it, so searching for one of them finds the archive, and
/// opening the result opens it in Zipling. Everything stays on this Mac.
enum Spotlight {
    static let domain = "com.observantsystems.zipling.archives"
    /// Names indexed per archive, at most.
    static let maxNames = 20_000

    static var enabled: Bool { Preferences.indexArchives }

    /// What Spotlight keeps about one archive.
    static func item(for archive: URL, listing: Listing) -> CSSearchableItem {
        let attributes = CSSearchableItemAttributeSet(contentType: .archive)
        attributes.title = archive.lastPathComponent
        let names = listing.entries.filter { !$0.isDirectory }.map(\.name)
        let shown = names.prefix(8).joined(separator: ", ")
        attributes.contentDescription = "\(Format.count(listing.files, "file")) inside" + (shown.isEmpty ? "" : ": \(shown)")
            + (names.count > 8 ? "…" : "")
        // Full paths, so a search for a folder inside finds it too.
        attributes.textContent = listing.entries.prefix(maxNames).map(\.path).joined(separator: "\n")
        attributes.keywords = Array(Set(names.prefix(200)))
        attributes.contentModificationDate = listing.createdDate
        attributes.relatedUniqueIdentifier = archive.path
        let item = CSSearchableItem(uniqueIdentifier: archive.path, domainIdentifier: domain, attributeSet: attributes)
        item.expirationDate = .distantFuture
        return item
    }

    static func index(_ archive: URL, listing: Listing) {
        guard enabled else { return }
        CSSearchableIndex.default().indexSearchableItems([item(for: archive, listing: listing)])
    }

    static func remove(_ archive: URL) {
        CSSearchableIndex.default().deleteSearchableItems(withIdentifiers: [archive.path])
    }

    /// Takes everything Zipling added out of Spotlight.
    static func removeAll() async {
        try? await CSSearchableIndex.default().deleteSearchableItems(withDomainIdentifiers: [domain])
    }

    /// The archive a Spotlight result stands for.
    static func archive(for activity: NSUserActivity) -> URL? {
        guard activity.activityType == CSSearchableItemActionType,
              let path = activity.userInfo?[CSSearchableItemActivityIdentifier] as? String else { return nil }
        return URL(filePath: path)
    }
}
