import SwiftUI

/// The home window: drop files to compress, or an archive to open.
struct HomeView: View {
    @Environment(\.openWindow) private var openWindow
    @State private var router = AppRouter.shared
    @State private var jobs = JobCenter.shared
    /// What's being dragged over the window, while it is.
    @State private var hover: DropHover?
    @State private var zone: CGRect = .zero
    private var targeted: Bool { hover != nil }

    var body: some View {
        @Bindable var router = router
        VStack(alignment: .leading, spacing: 24) {
            dropZone
            if !jobs.jobs.isEmpty {
                ActivityList(center: jobs)
            }
            if !router.recents.isEmpty {
                RecentList(urls: Array(router.recents.prefix(5))) { router.open([$0]) }
            }
            Text("Zipling engine \(Engine.version) · lossless · every byte verified")
                .font(.caption)
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity)
        }
        .padding(.horizontal, 28)
        .padding(.top, 36)
        .padding(.bottom, 20)
        .frame(width: 600)
        .coordinateSpace(.named(Self.space))
        .onDrop(of: [.fileURL], delegate: HomeDrop(hover: $hover, zone: zone))
        .sheet(item: $router.compressRequest) { request in
            CompressView(items: request.items)
        }
        .onAppear { router.attach(openWindow) }
    }

    private var dropZone: some View {
        VStack(spacing: 14) {
            ZStack {
                Circle()
                    .fill(.tint.opacity(targeted ? 0.22 : 0.12))
                    .frame(width: 96, height: 96)
                Image(systemName: targeted ? "tray.and.arrow.down.fill" : "shippingbox.fill")
                    .font(.system(size: 42))
                    .foregroundStyle(.tint)
                    .contentTransition(.symbolEffect(.replace))
            }
            Text(hover.map { $0.count > 1 ? "Compress \($0.count) items into one archive" : "Drop to compress" }
                 ?? "Drop files to compress")
                .font(.title2.weight(.semibold))
            Text("Or drop an archive to open it.")
                .font(.callout)
                .foregroundStyle(.secondary)
            HStack(spacing: 10) {
                Button {
                    router.chooseFilesToCompress()
                } label: {
                    Label("Choose Files…", systemImage: "plus")
                }
                .buttonStyle(.borderedProminent)
                Button {
                    router.chooseArchive()
                } label: {
                    Label("Open Archive…", systemImage: "folder")
                }
                Button {
                    router.chooseArchivesToExtract()
                } label: {
                    Label("Extract…", systemImage: "arrow.down.doc")
                }
                .help("Extract one or more archives, each next to itself")
            }
            .controlSize(.large)
            .padding(.top, 6)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
        .background {
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .fill(targeted ? AnyShapeStyle(.tint.opacity(0.08)) : AnyShapeStyle(.quaternary.opacity(0.35)))
        }
        .overlay {
            RoundedRectangle(cornerRadius: 22, style: .continuous)
                .strokeBorder(
                    targeted ? AnyShapeStyle(.tint) : AnyShapeStyle(.separator),
                    style: StrokeStyle(lineWidth: targeted ? 2 : 1, dash: [7, 5])
                )
        }
        .overlay {
            if let hover, hover.archives { ArchiveDropChoice(hover: hover) }
        }
        .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(Self.space)) } action: { zone = $0 }
        .animation(.snappy(duration: 0.2), value: hover)
    }

    private nonisolated static let space = "home"
}

/// Files over the home window: how many, whether they're all archives, and for
/// archives, whether the pointer is over Compress rather than Open.
struct DropHover: Equatable {
    var count: Int
    var archives: Bool
    var compress: Bool
}

/// Archives dropped on the drop zone's right part are compressed instead of opened.
private struct HomeDrop: DropDelegate {
    @Binding var hover: DropHover?
    let zone: CGRect
    /// The drop zone's share that opens archives; the rest compresses them.
    static let openShare: CGFloat = 0.6

    func validateDrop(info: DropInfo) -> Bool { info.hasItemsConforming(to: [.fileURL]) }

    func dropEntered(info: DropInfo) { hover = state(info) }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        hover = state(info)
        return DropProposal(operation: .copy)
    }

    func dropExited(info: DropInfo) { hover = nil }

    func performDrop(info: DropInfo) -> Bool {
        let urls = AppRouter.draggedFiles()
        let compress = state(info)?.compress ?? false
        hover = nil
        guard !urls.isEmpty else { return false }
        if compress { AppRouter.shared.compress(urls) } else { AppRouter.shared.open(urls) }
        return true
    }

    private func state(_ info: DropInfo) -> DropHover? {
        let urls = AppRouter.draggedFiles()
        guard !urls.isEmpty else { return nil }
        let archives = urls.allSatisfy(AppRouter.isArchive)
        let onCompress = zone.contains(info.location) && info.location.x >= zone.minX + zone.width * Self.openShare
        return DropHover(count: urls.count, archives: archives, compress: archives && onCompress)
    }
}

/// While archives are over the drop zone: Open on the left, Compress on the right.
private struct ArchiveDropChoice: View {
    let hover: DropHover

    var body: some View {
        GeometryReader { geometry in
            HStack(spacing: 0) {
                target("Open", symbol: "shippingbox",
                       detail: hover.count > 1 ? "Each in its own window" : "Browse what's inside",
                       active: !hover.compress)
                    .frame(width: geometry.size.width * HomeDrop.openShare)
                target("Compress", symbol: "archivebox",
                       detail: hover.count > 1 ? "All into one archive" : "Into a new archive",
                       active: hover.compress)
                    .frame(width: geometry.size.width * (1 - HomeDrop.openShare))
            }
        }
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
    }

    private func target(_ title: String, symbol: String, detail: String, active: Bool) -> some View {
        VStack(spacing: 8) {
            Image(systemName: symbol)
                .font(.system(size: 34))
                .foregroundStyle(active ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
            Text(title).font(.title3.weight(.semibold))
            Text(detail).font(.callout).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .fill(active ? AnyShapeStyle(.tint.opacity(0.14)) : AnyShapeStyle(.clear))
        }
        .overlay {
            RoundedRectangle(cornerRadius: 16, style: .continuous)
                .strokeBorder(active ? AnyShapeStyle(.tint) : AnyShapeStyle(.separator),
                              style: StrokeStyle(lineWidth: active ? 2 : 1, dash: [7, 5]))
        }
        .padding(8)
    }
}

/// Everything extracting, compressing or checking right now, and what just finished.
private struct ActivityList: View {
    let center: JobCenter

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Activity")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                if center.jobs.contains(where: { !$0.isRunning }) {
                    Button("Clear") { center.clearFinished() }
                        .buttonStyle(.borderless)
                        .font(.caption)
                }
            }
            .padding(.horizontal, 8)
            ForEach(center.jobs.prefix(5)) { job in
                JobRow(job: job) { center.dismiss(job) }
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
            }
        }
    }
}

private struct RecentList: View {
    let urls: [URL]
    let open: (URL) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Recent")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.leading, 8)
            ForEach(urls, id: \.self) { url in
                RecentRow(url: url) { open(url) }
            }
        }
    }
}

private struct RecentRow: View {
    let url: URL
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(nsImage: NSWorkspace.shared.icon(forFile: url.path))
                    .resizable()
                    .frame(width: 28, height: 28)
                VStack(alignment: .leading, spacing: 1) {
                    Text(url.deletingPathExtension().lastPathComponent)
                        .lineLimit(1)
                    Text(url.deletingLastPathComponent().path(percentEncoded: false))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                Spacer()
                if let size = try? url.resourceValues(forKeys: [.fileSizeKey]).fileSize {
                    Text(Format.bytes(UInt64(size)))
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(hovering ? AnyShapeStyle(.quaternary) : AnyShapeStyle(.clear))
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}
