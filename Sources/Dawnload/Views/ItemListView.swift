import SwiftUI

/// Filter tabs, the queue control and the list (or the empty state).
struct ItemListView: View {
    @EnvironmentObject private var model: AppModel
    var onOpenInspector: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            header
                .padding(.horizontal, DLSpace.gutter)
                .padding(.bottom, 10)

            Rectangle().fill(DLColor.rule).frame(height: 1)

            content
        }
    }

    // MARK: Header

    private var header: some View {
        HStack(spacing: 4) {
            ForEach(model.visibleFilters) { filter in
                FilterTab(filter: filter, count: model.count(filter), isSelected: model.filter == filter) {
                    model.filter = filter
                }
            }
            Spacer(minLength: 12)
            queueControl
            moreMenu
        }
    }

    @ViewBuilder
    private var queueControl: some View {
        if model.hasActive {
            Button {
                model.pauseAll()
            } label: {
                Label("Pause All", systemImage: "pause.fill").labelStyle(PillLabelStyle())
            }
            .buttonStyle(SecondaryPillStyle())
            .help("Pause All (⌘.)")
        } else if model.hasWaiting {
            Button {
                model.startQueue()
            } label: {
                Label("Start Queue", systemImage: "play.fill").labelStyle(PillLabelStyle())
            }
            .buttonStyle(SecondaryPillStyle(tone: DLColor.link))
            .help("Start Queue (⇧⌘S)")
        } else if model.hasPaused {
            Button {
                model.resumeAll()
            } label: {
                Label("Resume All", systemImage: "play.fill").labelStyle(PillLabelStyle())
            }
            .buttonStyle(SecondaryPillStyle())
        }
    }

    private var moreMenu: some View {
        MenuAnchor(entries: {
            [
                MenuChip.Entry(title: "Clear Kept", isEnabled: model.count(.kept) > 0) { model.clear(.kept) },
                MenuChip.Entry(title: "Clear Failed", isEnabled: model.count(.failed) > 0) { model.clear(.failed) },
                MenuChip.Entry(title: "Remove Waiting", isEnabled: model.hasWaiting) { model.clear(.queued) }
            ]
        }) {
            Image(systemName: "ellipsis")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(DLColor.text2)
                .frame(width: 28, height: 28)
                .background(Circle().strokeBorder(DLColor.rule))
                .contentShape(Circle())
        }
        .help("More")
        .accessibilityLabel("More")
    }

    // MARK: Content

    @ViewBuilder
    private var content: some View {
        let sections = model.sections()
        if model.items.isEmpty {
            EmptyHero()
        } else if sections.isEmpty {
            VStack {
                Spacer()
                Text(model.filter.emptyLine)
                    .font(.system(size: 13))
                    .foregroundStyle(DLColor.text2)
                Spacer()
            }
            .frame(maxWidth: .infinity)
        } else {
            list(sections)
        }
    }

    private func list(_ sections: [(ListSection, [DownloadItem])]) -> some View {
        List(selection: $model.selectedID) {
            ForEach(sections, id: \.0) { section, items in
                // A plain row, not a Section header: system headers are sticky grey bars.
                SectionLabel(title: section.title, trailing: "\(items.count)")
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.leading, 8)
                    .padding(.top, section == sections.first?.0 ? 6 : 14)
                    .padding(.bottom, 2)
                    .listRowInsets(EdgeInsets(top: 0, leading: 12, bottom: 0, trailing: 12))
                    .listRowBackground(DLColor.canvas)
                    .listRowSeparator(.hidden)
                ForEach(items) { item in
                    ItemRow(item: item, isSelected: model.selectedID == item.id)
                        .tag(item.id)
                        .listRowInsets(EdgeInsets(top: 1, leading: 12, bottom: 1, trailing: 12))
                        .listRowBackground(DLColor.canvas)
                        .listRowSeparator(.hidden)
                }
                .onMove(perform: section == .upNext ? { model.moveWaiting(fromOffsets: $0, toOffset: $1) } : nil)
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(DLColor.canvas)
        .environment(\.defaultMinListRowHeight, 20)
        .contextMenu(forSelectionType: UUID.self) { ids in
            RowMenu(ids: ids)
        } primaryAction: { ids in
            guard ids.count == 1, let id = ids.first, let item = model.items.first(where: { $0.id == id }) else { return }
            if item.status == .kept { model.open(item) } else { onOpenInspector() }
        }
        .onDeleteCommand {
            if let id = model.selectedID { model.remove([id]) }
        }
    }
}

private struct FilterTab: View {
    let filter: ListFilter
    let count: Int
    let isSelected: Bool
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Text(filter.title)
                    .font(.system(size: 12, weight: isSelected ? .semibold : .medium))
                    .foregroundStyle(isSelected ? DLColor.text : DLColor.text2)
                Text("\(count)")
                    .font(DLFont.mono(10, bold: true))
                    .foregroundStyle(count == 0 ? DLColor.text2.opacity(0.55) : (isSelected ? DLColor.link : DLColor.text2))
                    .monospacedDigit()
            }
            .padding(.horizontal, 10)
            .frame(height: 28)
            .background(Capsule().fill(isSelected ? DLColor.raised : (hovering ? DLColor.raised.opacity(0.5) : .clear)))
            .overlay(Capsule().strokeBorder(isSelected ? DLColor.rule : .clear))
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .accessibilityLabel("\(filter.title), \(count)")
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
}

struct PillLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 5) {
            configuration.icon.font(.system(size: 9, weight: .heavy))
            configuration.title
        }
    }
}

/// The context menu for one or more rows.
struct RowMenu: View {
    @EnvironmentObject private var model: AppModel
    let ids: Set<UUID>

    var body: some View {
        let items = model.items.filter { ids.contains($0.id) }
        if items.count == 1, let item = items.first {
            single(item)
        } else if !items.isEmpty {
            Button("Remove \(items.count) Items") { model.remove(ids) }
        }
    }

    @ViewBuilder
    private func single(_ item: DownloadItem) -> some View {
        switch item.status {
        case .kept:
            Button("Open") { model.open(item) }
            Button("Show in Finder") { model.reveal(item) }
        case .seeding:
            Button("Show in Finder") { model.reveal(item) }
            Button("Stop Seeding") { model.stopSeeding(item) }
        case .pulling, .polishing:
            Button("Pause") { model.pause(item) }
        case .queued:
            Button("Download Now") { model.runSoon(item) }
            Button("Move to Top") { model.moveToTop(item) }
        case .paused:
            Button("Resume") { model.runSoon(item) }
        case .failed:
            Button("Retry") { model.runSoon(item) }
        }
        Divider()
        Button("Copy Link") { model.copyLink(item) }
        Button("Copy Log") { model.copyLog(item) }.disabled(item.log.isEmpty)
        Divider()
        Button("Remove") { model.remove([item.id]) }
    }
}
