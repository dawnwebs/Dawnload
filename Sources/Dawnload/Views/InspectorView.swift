import DawnloadCore
import SwiftUI

/// Everything about the selected item: status, progress, swarm, files, options, log, actions.
struct InspectorView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Group {
            if let item = model.selectedItem {
                ItemInspector(item: item)
                    .id(item.id)
            } else {
                VStack(spacing: 12) {
                    CutDGlyph(cut: .dots, style: .outline, color: DLColor.track)
                        .frame(width: 34, height: 44)
                        .accessibilityHidden(true)
                    Text("Select a download to see its files, options and log.")
                        .font(.system(size: 12))
                        .foregroundStyle(DLColor.text2)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: 200)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(DLColor.raised.opacity(0.55).ignoresSafeArea())
    }
}

private struct ItemInspector: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var item: DownloadItem

    var body: some View {
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    status
                    if item.status == .pulling || item.status == .paused { progress }
                    if let swarm = item.swarm { swarmSection(swarm) }
                    if !item.files.isEmpty { filesSection }
                    optionsSection
                    logSection
                }
                .padding(20)
            }
            Rectangle().fill(DLColor.rule).frame(height: 1)
            actions
                .padding(.horizontal, 20)
                .padding(.vertical, 14)
        }
    }

    // MARK: Status

    private var status: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                StatusGlyph(status: item.status, fill: item.fill, indeterminate: item.isIndeterminate)
                    .frame(width: 34, height: 44)
                VStack(alignment: .leading, spacing: 2) {
                    Text(item.status.word)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(item.statusColor)
                    if let detail = item.phaseDetail, item.status.isActive {
                        Text(detail)
                            .font(DLFont.mono(10))
                            .foregroundStyle(DLColor.text2)
                    }
                }
            }

            Text(item.displayTitle)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(DLColor.text)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)

            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(item.source.absoluteString)
                    .font(DLFont.mono(10))
                    .foregroundStyle(DLColor.text2)
                    .lineLimit(2)
                    .truncationMode(.middle)
                    .textSelection(.enabled)
                Spacer(minLength: 4)
                Button("Copy") { model.copyLink(item) }
                    .buttonStyle(LinkButtonStyle())
                    .help("Copy Link")
            }

            if let failure = item.failure, item.status == .failed {
                VStack(alignment: .leading, spacing: 6) {
                    Text(failure.message)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(DLColor.failed)
                    if let hint = failure.hint {
                        Text(hint)
                            .font(.system(size: 12))
                            .foregroundStyle(DLColor.text)
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .dlSurface(fill: DLColor.canvas)
            }
        }
    }

    // MARK: Progress

    private var progress: some View {
        let t = item.transfer
        return VStack(alignment: .leading, spacing: 10) {
            if let fraction = t.fraction {
                Text(DLFormat.percent(fraction))
                    .font(DLFont.display(34))
                    .tracking(DLFont.displayTracking(34))
                    .foregroundStyle(item.status == .paused ? DLColor.text2 : DLColor.text)
                    .monospacedDigit()
                    .accessibilityLabel("\(Int(fraction * 100)) percent")
            }
            VStack(spacing: 6) {
                if let index = t.entryIndex, let count = t.entryCount, count > 1 {
                    FactRow(label: "Playlist", value: "Video \(index) of \(count)")
                }
                if let done = t.bytesDone {
                    FactRow(label: "Size", value: t.bytesTotal.map { DLFormat.bytes(done, of: $0) } ?? DLFormat.bytes(done))
                }
                if let speed = t.bytesPerSecond, speed > 0 {
                    FactRow(label: "Speed", value: DLFormat.speed(speed))
                }
                if let left = t.secondsLeft {
                    FactRow(label: "Time left", value: DLFormat.clock(left))
                }
            }
        }
    }

    private func swarmSection(_ swarm: SwarmStats) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionLabel(title: "Swarm")
            VStack(spacing: 6) {
                FactRow(label: "Peers", value: "\(swarm.peers)")
                FactRow(label: "Seeds", value: "\(swarm.seeds)")
                if swarm.downloadRate > 0 { FactRow(label: "Down", value: DLFormat.speed(swarm.downloadRate)) }
                FactRow(label: "Up", value: DLFormat.speed(swarm.uploadRate))
                FactRow(label: "Uploaded", value: DLFormat.bytes(swarm.uploaded))
                FactRow(label: "Ratio", value: DLFormat.ratio(swarm.ratio),
                        valueColor: swarm.ratio >= (item.options.seedRatioLimit ?? .infinity) ? DLColor.kept : DLColor.text)
            }
        }
    }

    // MARK: Files

    private var filesSection: some View {
        let selectable = EngineRegistry.engine(item.engine).capabilities.contains(.fileSelection) && item.status.isEditable
        let selected = item.keptFiles.count
        return VStack(alignment: .leading, spacing: 8) {
            SectionLabel(title: "Files", trailing: selectable || selected != item.files.count
                ? "\(selected) of \(item.files.count)" : "\(item.files.count)")
            VStack(spacing: 2) {
                ForEach(item.files.prefix(200)) { file in
                    FileRow(file: file, selectable: selectable, revealable: item.status.hasFiles) {
                        model.toggleFile(file, of: item)
                    }
                }
                if item.files.count > 200 {
                    Text("and \(item.files.count - 200) more")
                        .font(.system(size: 11))
                        .foregroundStyle(DLColor.text2)
                        .padding(.top, 4)
                }
            }
        }
    }

    // MARK: Options

    private var optionsSection: some View {
        let editable = item.status.isEditable
        let caps = EngineRegistry.engine(item.engine).capabilities
        return VStack(alignment: .leading, spacing: 10) {
            SectionLabel(title: "Options", trailing: item.status.isActive || item.status == .seeding ? "read-only while it runs" : nil)
            VStack(spacing: 8) {
                FactRow(label: "Engine", value: EngineRegistry.engine(item.engine).displayName)
                if editable && !item.source.isTorrent {
                    Picker("Engine", selection: Binding(
                        get: { item.options.engine },
                        set: { value in model.update(item) { $0.engine = value } }
                    )) {
                        Text("Auto").tag(EnginePreference.auto)
                        Text("yt-dlp").tag(EnginePreference.ytDlp)
                        Text("gallery-dl").tag(EnginePreference.galleryDl)
                    }
                    .pickerStyle(.segmented)
                    .labelsHidden()
                }
                if caps.contains(.formats) {
                    if editable {
                        Picker("Format", selection: Binding(
                            get: { item.options.quality },
                            set: { value in model.update(item) { $0.quality = value } }
                        )) {
                            ForEach(QualityPreference.allCases) { Text($0.title).tag($0) }
                        }
                        .pickerStyle(.segmented)
                        .labelsHidden()
                    } else {
                        FactRow(label: "Format", value: item.options.quality.title)
                    }
                }
                if caps.contains(.polish) {
                    optionToggle("Polish with ffmpeg", \.polish, editable: editable)
                }
                if caps.contains(.playlists) {
                    optionToggle("Whole playlist", \.playlist, editable: editable)
                }
                if caps.contains(.siteAccess) {
                    optionToggle(item.options.impersonate ? "Impersonate \(item.options.impersonationTarget.title)" : "Impersonate a browser",
                                 \.impersonate, editable: editable)
                    if item.options.cookiesFile != nil || model.cookiesFile != nil {
                        optionToggle("Send cookies.txt", \.useCookies, editable: editable)
                    }
                }
                if caps.contains(.seeding) {
                    FactRow(label: "Seed until ratio", value: item.options.seedRatioLimit.map(DLFormat.ratio) ?? "Stopped by you")
                }
                FactRow(label: "Folder", value: item.options.destination.path)
            }
            .tint(DLColor.link)
            .disabled(!editable)
        }
    }

    private func optionToggle(_ title: String, _ keyPath: WritableKeyPath<DownloadOptions, Bool>, editable: Bool) -> some View {
        HStack {
            Text(title)
                .font(.system(size: 12))
                .foregroundStyle(DLColor.text2)
            Spacer(minLength: 8)
            Toggle(title, isOn: Binding(
                get: { item.options[keyPath: keyPath] },
                set: { value in model.update(item) { $0[keyPath: keyPath] = value } }
            ))
            .labelsHidden()
            .toggleStyle(.switch)
            .controlSize(.mini)
        }
    }

    // MARK: Log

    private var logSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                SectionLabel(title: "Log")
                Spacer()
                Button("Copy Log") { model.copyLog(item) }
                    .buttonStyle(LinkButtonStyle())
                    .disabled(item.log.isEmpty)
            }
            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 3) {
                        if item.log.isEmpty {
                            Text("The tool's output appears here once it starts.")
                                .font(.system(size: 11))
                                .foregroundStyle(DLColor.text2)
                        }
                        ForEach(Array(item.log.enumerated()), id: \.offset) { index, line in
                            Text(line)
                                .font(DLFont.mono(10))
                                .foregroundStyle(line.hasPrefix("$ ") || line.hasPrefix("ERROR") || line.hasPrefix("Failed")
                                    ? (line.hasPrefix("$ ") ? DLColor.text2 : DLColor.failed)
                                    : DLColor.text)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .id(index)
                        }
                    }
                    .textSelection(.enabled)
                    .padding(10)
                }
                .frame(height: 180)
                .dlSurface(fill: DLColor.canvas)
                .onChange(of: item.log.count) { count in
                    proxy.scrollTo(count - 1, anchor: .bottom)
                }
                .onAppear { proxy.scrollTo(item.log.count - 1, anchor: .bottom) }
            }
        }
    }

    // MARK: Actions

    private var actions: some View {
        HStack(spacing: 8) {
            switch item.status {
            case .kept:
                Button("Show in Finder") { model.reveal(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30, tone: DLColor.link))
                Button("Open") { model.open(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30))
            case .seeding:
                Button("Stop Seeding") { model.stopSeeding(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30, tone: DLColor.link))
                Button("Show in Finder") { model.reveal(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30))
            case .pulling, .polishing:
                Button("Pause") { model.pause(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30, tone: DLColor.link))
            case .queued:
                Button("Download Now") { model.runSoon(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30, tone: DLColor.link))
            case .paused:
                Button("Resume") { model.runSoon(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30, tone: DLColor.link))
            case .failed:
                Button("Retry") { model.runSoon(item) }
                    .buttonStyle(SecondaryPillStyle(height: 30, tone: DLColor.link))
            }
            Spacer(minLength: 4)
            Button {
                model.remove([item.id])
            } label: {
                Image(systemName: "trash")
            }
            .buttonStyle(RoundIconButtonStyle(size: 30))
            .help("Remove from the list. Kept files stay on disk.")
            .accessibilityLabel("Remove")
        }
    }
}

private struct FileRow: View {
    var file: ItemFile
    var selectable: Bool
    var revealable: Bool
    var onToggle: () -> Void
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 8) {
            if selectable {
                Toggle("", isOn: Binding(get: { file.isSelected }, set: { _ in onToggle() }))
                    .toggleStyle(.checkbox)
                    .labelsHidden()
                    .tint(DLColor.link)
            } else {
                Image(systemName: icon)
                    .font(.system(size: 11))
                    .foregroundStyle(file.isSelected ? DLColor.text2 : DLColor.text2.opacity(0.4))
                    .frame(width: 14)
            }
            Text(file.name)
                .font(DLFont.mono(11))
                .foregroundStyle(file.isSelected ? DLColor.text : DLColor.text2.opacity(0.6))
                .strikethrough(!file.isSelected && !selectable)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 6)
            if let size = file.size {
                Text(DLFormat.bytes(size))
                    .font(DLFont.mono(10))
                    .foregroundStyle(DLColor.text2)
            }
        }
        .padding(.horizontal, 8)
        .frame(height: 26)
        .background(RoundedRectangle(cornerRadius: DLRadius.spine).fill(hovering && revealable ? DLColor.raisedHover : .clear))
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .onTapGesture {
            if revealable, FileManager.default.fileExists(atPath: file.path) {
                NSWorkspace.shared.activateFileViewerSelecting([file.url])
            }
        }
        .help(revealable ? "Show \(file.name) in Finder" : file.path)
    }

    private var icon: String {
        switch file.url.pathExtension.lowercased() {
        case "mp4", "m4v", "mov", "mkv", "webm", "avi": "film"
        case "m4a", "mp3", "aac", "opus", "flac", "wav", "ogg": "waveform"
        case "jpg", "jpeg", "png", "gif", "webp", "heic", "avif": "photo"
        default: "doc"
        }
    }
}
