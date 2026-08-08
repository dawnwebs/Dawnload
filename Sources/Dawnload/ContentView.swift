import AppKit
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var model: DownloadViewModel
    @State private var activeDrawer: DrawerPanel?
    @State private var isUrlEditing = false
    @State private var inputFocusTrigger = 0

    var body: some View {
        ZStack {
            appBackground

            ZStack {
                mainStage
                topChrome
                bottomNote
                bottomMenu
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            drawerLayer
        }
        .foregroundStyle(DawnloadColors.text)
        .font(.system(size: 14, weight: .medium, design: .monospaced))
        .background(
            WindowBackgroundConfigurator(isGlassEnabled: model.useGlassBackground)
                .allowsHitTesting(false)
        )
    }

    // MARK: - Layout

    @ViewBuilder
    private var appBackground: some View {
        if model.useGlassBackground {
            ZStack {
                Color.clear.ignoresSafeArea()

                Rectangle()
                    .fill(.ultraThinMaterial)
                    .ignoresSafeArea()

                Color.black.opacity(0.08)
                    .ignoresSafeArea()

                RadialGradient(
                    colors: [DawnloadColors.blue.opacity(0.16), .clear],
                    center: UnitPoint(x: 0.5, y: 0.36),
                    startRadius: 0,
                    endRadius: 420
                )
                .ignoresSafeArea()
            }
        } else {
            DawnloadColors.stage.ignoresSafeArea()
        }
    }

    private var mainStage: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 98)

            VStack(spacing: 18) {
                DawnloadSigil(state: model.state, progress: model.progress)
                    .frame(width: 142, height: 126)
                    .padding(.bottom, 2)

                omnibox
                    .frame(maxWidth: 640)

                actionStrip
                    .frame(maxWidth: 640)

                stateLine
                    .frame(maxWidth: 640)
            }
            .padding(.horizontal, 34)

            Spacer(minLength: 128)
        }
    }

    private var topChrome: some View {
        VStack {
            HStack(alignment: .top) {
                Spacer()

                Button {
                    toggleDrawer(.tools)
                } label: {
                    HStack(spacing: 9) {
                        Image(systemName: toolsReady ? "checkmark.seal.fill" : "exclamationmark.triangle.fill")
                            .font(.system(size: 13, weight: .bold))
                            .foregroundStyle(toolsReady ? DawnloadColors.green : DawnloadColors.orange)
                            .frame(width: 22, height: 22)
                            .background(Circle().fill(DawnloadColors.glassTint))

                        Text(toolSummary)
                            .font(.system(size: 13, weight: .semibold, design: .monospaced))
                    }
                    .padding(.horizontal, 10)
                    .frame(height: 34)
                    .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .foregroundStyle(DawnloadColors.muted)
                .background(
                    Capsule()
                        .fill(activeDrawer == .tools ? DawnloadColors.control : Color.clear)
                )

                Spacer()
            }
            .overlay(alignment: .trailing) {
                QueueStatusButton(
                    state: model.state,
                    progress: model.progress,
                    queuedCount: model.queue.count,
                    isActive: activeDrawer == .queue
                ) {
                    toggleDrawer(.queue)
                }
                .padding(.trailing, 22)
            }
            .padding(.top, 18)

            Spacer()
        }
        .allowsHitTesting(true)
    }

    private var bottomNote: some View {
        VStack {
            Spacer()

            HStack(spacing: 6) {
                Text("saving to")
                    .foregroundStyle(DawnloadColors.dim)

                Button {
                    model.chooseDestination()
                } label: {
                    Text(model.destination.path)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .underline()
                }
                .buttonStyle(.plain)
                .foregroundStyle(DawnloadColors.muted)
                .disabled(model.state.isRunning)
            }
            .font(.system(size: 12, weight: .medium, design: .monospaced))
            .frame(maxWidth: 680)
            .padding(.horizontal, 34)
            .padding(.bottom, 104)
        }
    }

    private var bottomMenu: some View {
        VStack {
            Spacer()

            HStack(spacing: 6) {
                DockButton(
                    title: "save",
                    systemImage: "arrow.down.to.line",
                    isActive: activeDrawer == nil
                ) {
                    withAnimation(.spring(response: 0.28, dampingFraction: 0.88)) {
                        activeDrawer = nil
                    }
                }

                DockButton(
                    title: "queue",
                    systemImage: "list.bullet.rectangle",
                    badge: model.queue.isEmpty ? nil : "\(model.queue.count)",
                    isActive: activeDrawer == .queue
                ) {
                    toggleDrawer(.queue)
                }

                DockButton(
                    title: "tools",
                    systemImage: "wrench.and.screwdriver",
                    isActive: activeDrawer == .tools
                ) {
                    toggleDrawer(.tools)
                }

                Divider()
                    .frame(height: 28)
                    .overlay(DawnloadColors.glassStroke)
                    .padding(.horizontal, 3)

                DockButton(
                    title: "settings",
                    systemImage: "gearshape",
                    isActive: activeDrawer == .settings
                ) {
                    toggleDrawer(.settings)
                }

                DockButton(
                    title: "activity",
                    systemImage: "terminal",
                    isActive: activeDrawer == .activity
                ) {
                    toggleDrawer(.activity)
                }

                DockButton(
                    title: "about",
                    systemImage: "info.circle",
                    isActive: activeDrawer == .about
                ) {
                    toggleDrawer(.about)
                }
            }
            .padding(8)
            .background(.ultraThinMaterial, in: Capsule())
            .background(
                Capsule()
                    .fill(DawnloadColors.glassTint)
            )
            .overlay(
                Capsule()
                    .stroke(
                        LinearGradient(
                            colors: [Color.white.opacity(0.28), Color.white.opacity(0.06), Color.white.opacity(0.16)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ),
                        lineWidth: 1
                    )
            )
            .shadow(color: .black.opacity(0.46), radius: 30, y: 16)
            .padding(.horizontal, 22)
            .padding(.bottom, 18)
        }
        .allowsHitTesting(true)
    }

    // MARK: - Omnibox

    private var omnibox: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                ZStack {
                    Image(systemName: "link")
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(inputAccent)
                }
                .frame(width: 48, height: 52)

                NativeURLTextField(
                    text: $model.urlText,
                    placeholder: "paste the link here",
                    isDisabled: model.state.isRunning,
                    isEditing: $isUrlEditing,
                    focusTrigger: inputFocusTrigger,
                    onSubmit: { if model.canStart { model.startDownload() } }
                )
                .frame(height: 52)
                .layoutPriority(2)

                if !model.urlText.isEmpty, !model.state.isRunning {
                    Button {
                        model.urlText = ""
                        inputFocusTrigger += 1
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 11, weight: .bold))
                            .frame(width: 26, height: 26)
                            .background(Circle().fill(DawnloadColors.control))
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(DawnloadColors.muted)
                    .padding(.trailing, 8)
                }

                Button {
                    model.startDownload()
                } label: {
                    Text(downloadButtonTitle)
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))
                        .frame(width: 64, height: 52)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(model.canStart ? DawnloadColors.text : DawnloadColors.dim)
                .disabled(!model.canStart)
                .keyboardShortcut(.defaultAction)
                .overlay(alignment: .leading) {
                    Rectangle()
                        .fill(inputBorder)
                        .frame(width: isUrlEditing ? 2 : 1.5)
                }
            }

            if model.state.isRunning {
                ProgressLine(progress: model.progress, tone: inputAccent)
                    .frame(height: 3)
                    .transition(.opacity)
            }
        }
        .background(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .fill(DawnloadColors.stage)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 11, style: .continuous)
                .stroke(inputBorder, lineWidth: isUrlEditing ? 2 : 1.5)
        )
        .shadow(color: inputAccent.opacity(isUrlEditing || model.state.isRunning ? 0.28 : 0), radius: 24)
        .animation(.easeOut(duration: 0.18), value: isUrlEditing)
        .animation(.easeOut(duration: 0.18), value: model.state)
    }

    private var actionStrip: some View {
        VStack(alignment: .leading, spacing: 8) {
            backendToggle

            HStack(alignment: .top, spacing: 10) {
                qualityControls

                Spacer(minLength: 8)

                HStack(spacing: 8) {
                    Button {
                        if model.addCurrentToQueue() {
                            inputFocusTrigger += 1
                            withAnimation(.spring(response: 0.28, dampingFraction: 0.88)) {
                                activeDrawer = .queue
                            }
                        }
                    } label: {
                        Label("queue", systemImage: "text.badge.plus")
                    }
                    .buttonStyle(CobaltButtonStyle())
                    .disabled(!model.canAddToQueue)

                    Button {
                        pasteFromClipboard()
                    } label: {
                        Label("paste", systemImage: "doc.on.clipboard")
                    }
                    .buttonStyle(CobaltButtonStyle(isPrimary: false))
                    .disabled(model.state.isRunning)
                }
                .fixedSize(horizontal: true, vertical: false)
            }

            HStack(spacing: 8) {
                TogglePill(
                    title: "ffmpeg",
                    systemImage: "wand.and.stars",
                    isOn: $model.useFFmpeg,
                    disabled: model.state.isRunning
                )

                TogglePill(
                    title: model.impersonationTarget.compactTitle,
                    systemImage: "person.crop.circle.badge.checkmark",
                    isOn: $model.useBrowserImpersonation,
                    disabled: model.selectedMode == .galleryDl || model.state.isRunning
                )

                TogglePill(
                    title: "playlist",
                    systemImage: "list.bullet.rectangle",
                    isOn: $model.useYesPlaylist,
                    disabled: model.selectedMode == .galleryDl || model.state.isRunning
                )

                Spacer(minLength: 0)
            }
        }
    }

    private var backendToggle: some View {
        HStack(spacing: 0) {
            BackendToggleButton(
                title: "yt-dlp",
                detail: "video + audio",
                systemImage: "play.rectangle.stack",
                isActive: model.selectedMode == .ytDlp,
                disabled: model.state.isRunning
            ) {
                selectBackend(.ytDlp)
            }

            BackendToggleButton(
                title: "gallery-dl",
                detail: "galleries",
                systemImage: "photo.stack",
                isActive: model.selectedMode == .galleryDl,
                disabled: model.state.isRunning
            ) {
                selectBackend(.galleryDl)
            }
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .fill(DawnloadColors.control)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .stroke(DawnloadColors.hairline)
        )
    }

    @ViewBuilder
    private var qualityControls: some View {
        if model.selectedMode == .galleryDl {
            HStack(spacing: 7) {
                Image(systemName: "photo.on.rectangle")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(DawnloadColors.green)

                VStack(alignment: .leading, spacing: 1) {
                    Text("gallery mode")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))
                        .foregroundStyle(DawnloadColors.text)

                    Text("quality handled by gallery-dl")
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                }
            }
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .fill(DawnloadColors.control)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .stroke(DawnloadColors.green.opacity(0.28))
            )
            .fixedSize(horizontal: true, vertical: false)
        } else {
            HStack(spacing: 0) {
                ModeButton(kind: .auto, isActive: selectedChip == .auto, disabled: model.state.isRunning) {
                    select(.auto)
                }

                ModeButton(kind: .video, isActive: selectedChip == .video, disabled: model.state.isRunning) {
                    select(.video)
                }

                ModeButton(kind: .audio, isActive: selectedChip == .audio, disabled: model.state.isRunning) {
                    select(.audio)
                }
            }
            .fixedSize(horizontal: true, vertical: false)
            .background(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .fill(DawnloadColors.control)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 11, style: .continuous)
                    .stroke(DawnloadColors.hairline)
            )
        }
    }

    private var stateLine: some View {
        HStack(spacing: 9) {
            Image(systemName: model.state.symbolName)
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle(inputAccent)

            Text(model.state.title.lowercased())
                .foregroundStyle(DawnloadColors.text)

            Text("/")
                .foregroundStyle(DawnloadColors.dim)

            Text(model.statusDetail)
                .foregroundStyle(DawnloadColors.muted)
                .lineLimit(1)
                .truncationMode(.middle)

            Spacer(minLength: 0)

            if model.state.isRunning {
                Button("cancel") {
                    model.cancelDownload()
                }
                .buttonStyle(CobaltTextButtonStyle(tone: DawnloadColors.orange))
            }
        }
        .font(.system(size: 12, weight: .medium, design: .monospaced))
        .padding(.horizontal, 2)
    }

    // MARK: - Drawers

    private var drawerLayer: some View {
        ZStack(alignment: .trailing) {
            if let activeDrawer {
                Color.black.opacity(0.34)
                    .ignoresSafeArea()
                    .transition(.opacity)
                    .onTapGesture {
                        withAnimation(.spring(response: 0.32, dampingFraction: 0.9)) {
                            self.activeDrawer = nil
                        }
                    }

                DrawerShell(panel: activeDrawer, model: model) {
                    withAnimation(.spring(response: 0.32, dampingFraction: 0.9)) {
                        self.activeDrawer = nil
                    }
                }
                .frame(width: 376)
                .padding(16)
                .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        .animation(.spring(response: 0.32, dampingFraction: 0.9), value: activeDrawer)
    }

    // MARK: - Derived

    private var selectedChip: ModeChipKind {
        if model.selectedMode == .galleryDl {
            return .gallery
        }

        switch model.selectedQuality {
        case .audio:
            return .audio
        case .compatible:
            return .video
        case .best:
            return .auto
        }
    }

    private var inputAccent: Color {
        switch model.state {
        case .failed:
            return DawnloadColors.red
        case .completed:
            return DawnloadColors.green
        case .cancelled:
            return DawnloadColors.orange
        case .queued:
            return DawnloadColors.orange
        default:
            return DawnloadColors.blue
        }
    }

    private var inputBorder: Color {
        if isUrlEditing || model.canStart || model.state.isRunning {
            return inputAccent
        }

        return DawnloadColors.inputBorder
    }

    private var downloadButtonTitle: String {
        switch model.state {
        case .failed:
            return "retry"
        case .completed:
            return "done"
        default:
            return "save"
        }
    }

    private var toolSummary: String {
        let installed = model.tools.filter(\.isInstalled).count
        return "\(installed)/\(max(model.tools.count, 3)) tools ready"
    }

    private var toolsReady: Bool {
        !model.tools.isEmpty && model.tools.allSatisfy(\.isInstalled)
    }

    private func select(_ chip: ModeChipKind) {
        guard !model.state.isRunning else { return }

        switch chip {
        case .auto:
            model.selectedMode = .ytDlp
            model.selectedQuality = .best
        case .video:
            model.selectedMode = .ytDlp
            model.selectedQuality = .compatible
        case .audio:
            model.selectedMode = .ytDlp
            model.selectedQuality = .audio
        case .gallery:
            model.selectedMode = .galleryDl
        }
    }

    private func selectBackend(_ mode: DownloadMode) {
        guard !model.state.isRunning else { return }

        model.selectedMode = mode
    }

    private func toggleDrawer(_ drawer: DrawerPanel) {
        withAnimation(.spring(response: 0.28, dampingFraction: 0.88)) {
            activeDrawer = activeDrawer == drawer ? nil : drawer
        }
    }

    private func pasteFromClipboard() {
        guard !model.state.isRunning else { return }
        guard let text = NSPasteboard.general.string(forType: .string) else { return }

        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        model.urlText = trimmed
        inputFocusTrigger += 1
    }
}

// MARK: - Drawer content

private enum DrawerPanel: String, Identifiable {
    case queue
    case tools
    case settings
    case activity
    case about

    var id: String { rawValue }

    var title: String {
        switch self {
        case .queue:
            return "processing queue"
        case .tools:
            return "tool health"
        case .settings:
            return "settings"
        case .activity:
            return "activity"
        case .about:
            return "about dawnload"
        }
    }

    var icon: String {
        switch self {
        case .queue:
            return "arrow.down.circle"
        case .tools:
            return "wrench.and.screwdriver"
        case .settings:
            return "gearshape"
        case .activity:
            return "terminal"
        case .about:
            return "info.circle"
        }
    }
}

private struct DrawerShell: View {
    let panel: DrawerPanel
    @ObservedObject var model: DownloadViewModel
    let onClose: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                Image(systemName: panel.icon)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(DawnloadColors.muted)

                Text(panel.title)
                    .font(.system(size: 17, weight: .semibold, design: .monospaced))
                    .lineLimit(1)

                Spacer()

                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .bold))
                        .frame(width: 30, height: 30)
                }
                .buttonStyle(.plain)
                .foregroundStyle(DawnloadColors.muted)
                .background(Circle().fill(DawnloadColors.control))
            }

            switch panel {
            case .queue:
                QueueDrawer(model: model)
            case .tools:
                ToolsDrawer(model: model)
            case .settings:
                SettingsDrawer(model: model)
            case .activity:
                ActivityDrawer(model: model)
            case .about:
                AboutDrawer()
            }
        }
        .padding(16)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .fill(DawnloadColors.drawer)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(DawnloadColors.hairline, lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.45), radius: 34, x: -8, y: 16)
    }
}

private struct QueueDrawer: View {
    @ObservedObject var model: DownloadViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            DrawerCard {
                HStack(spacing: 12) {
                    ZStack {
                        Circle()
                            .stroke(DawnloadColors.inputBorder, lineWidth: 4)
                            .frame(width: 48, height: 48)

                        Circle()
                            .trim(from: 0, to: CGFloat(model.progress ?? (model.state.isRunning ? 0.24 : 1)))
                            .stroke(progressTone, style: StrokeStyle(lineWidth: 4, lineCap: .round))
                            .rotationEffect(.degrees(-90))
                            .frame(width: 48, height: 48)

                        Image(systemName: model.state.symbolName)
                            .foregroundStyle(progressTone)
                    }

                    VStack(alignment: .leading, spacing: 3) {
                        Text(model.state.title.lowercased())
                            .font(.system(size: 14, weight: .semibold, design: .monospaced))

                        Text(model.statusDetail)
                            .font(.system(size: 12, weight: .medium, design: .monospaced))
                            .foregroundStyle(DawnloadColors.muted)
                            .lineLimit(2)
                    }

                    Spacer()
                }

                HStack {
                    Button("start queue") {
                        model.startQueue()
                    }
                    .buttonStyle(CobaltButtonStyle())
                    .disabled(!model.canStartQueue)

                    Button("clear") {
                        model.clearQueue()
                    }
                    .buttonStyle(CobaltButtonStyle(isPrimary: false))
                    .disabled(model.queue.isEmpty || model.state.isRunning)

                    if model.state.isRunning {
                        Button("cancel") {
                            model.cancelDownload()
                        }
                        .buttonStyle(CobaltTextButtonStyle(tone: DawnloadColors.orange))
                    }
                }
            }

            if model.queue.isEmpty {
                EmptyState(
                    systemImage: "tray",
                    title: "queue is empty",
                    detail: "Paste a URL, choose its mode, then add it here for sequential downloads."
                )
            } else {
                ScrollView {
                    VStack(spacing: 8) {
                        ForEach(Array(model.queue.enumerated()), id: \.element.id) { index, item in
                            QueueRow(
                                item: item,
                                index: index + 1,
                                isActive: model.activeQueueItemID == item.id,
                                canRemove: !model.state.isRunning
                            ) {
                                model.removeQueuedItem(item)
                            }
                        }
                    }
                    .padding(.bottom, 4)
                }
            }
        }
    }

    private var progressTone: Color {
        switch model.state {
        case .failed:
            return DawnloadColors.red
        case .completed:
            return DawnloadColors.green
        case .cancelled, .queued:
            return DawnloadColors.orange
        default:
            return DawnloadColors.blue
        }
    }
}

private struct ToolsDrawer: View {
    @ObservedObject var model: DownloadViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            DrawerCard {
                HStack {
                    Text("command-line tools")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    Spacer()

                    Button {
                        model.refreshTools()
                    } label: {
                        Label("refresh", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(CobaltButtonStyle(isPrimary: false))
                }

                VStack(spacing: 10) {
                    ForEach(model.tools) { tool in
                        ToolRow(tool: tool)
                    }
                }
            }

            EmptyState(
                systemImage: "sparkles",
                title: "auto mode prefers yt-dlp",
                detail: "Gallery mode switches to gallery-dl. ffmpeg is used for merging, remuxing, and final polish when available."
            )
        }
    }
}

private struct SettingsDrawer: View {
    @ObservedObject var model: DownloadViewModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                DrawerCard {
                    Text("destination")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    Text(model.destination.path)
                        .font(.system(size: 12, weight: .medium, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                        .lineLimit(3)
                        .truncationMode(.middle)
                        .textSelection(.enabled)

                    Button("set default folder") {
                        model.chooseDestination()
                    }
                    .buttonStyle(CobaltButtonStyle())
                    .disabled(model.state.isRunning)
                }

                DrawerCard {
                    Text("run defaults")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    VStack(spacing: 8) {
                        DrawerToggle(title: "ffmpeg polish", systemImage: "wand.and.stars", isOn: $model.useFFmpeg, disabled: model.state.isRunning)
                        DrawerToggle(title: "browser impersonation", systemImage: "person.crop.circle.badge.checkmark", isOn: $model.useBrowserImpersonation, disabled: model.selectedMode == .galleryDl || model.state.isRunning)
                        DrawerToggle(title: "download playlist", systemImage: "list.bullet.rectangle", isOn: $model.useYesPlaylist, disabled: model.selectedMode == .galleryDl || model.state.isRunning)
                    }

                    HStack(spacing: 8) {
                        Text("browser profile")
                            .font(.system(size: 12, weight: .medium, design: .monospaced))
                            .foregroundStyle(DawnloadColors.muted)

                        Spacer()

                        Picker(
                            "browser profile",
                            selection: Binding(
                                get: { model.impersonationTarget },
                                set: { model.selectImpersonationTarget($0) }
                            )
                        ) {
                            ForEach(BrowserImpersonationTarget.allCases) { target in
                                Text(target.title).tag(target)
                            }
                        }
                        .labelsHidden()
                        .pickerStyle(.menu)
                        .disabled(
                            !model.useBrowserImpersonation
                                || model.selectedMode == .galleryDl
                                || model.state.isRunning
                        )
                    }
                }

                DrawerCard {
                    Text("cookies for blocked sites")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    if let cookiesFile = model.cookiesFile {
                        HStack(spacing: 8) {
                            Image(systemName: "doc.text.fill")
                                .foregroundStyle(DawnloadColors.green)

                            Text(cookiesFile.lastPathComponent)
                                .font(.system(size: 12, weight: .medium, design: .monospaced))
                                .lineLimit(1)
                                .truncationMode(.middle)

                            Spacer()

                            Button("remove") {
                                model.clearCookiesFile()
                            }
                            .buttonStyle(CobaltTextButtonStyle(tone: DawnloadColors.orange))
                            .disabled(model.state.isRunning)
                        }
                    } else {
                        Text("No cookies file selected.")
                            .font(.system(size: 12, weight: .medium, design: .monospaced))
                            .foregroundStyle(DawnloadColors.muted)
                    }

                    Button(model.cookiesFile == nil ? "select cookies.txt" : "replace cookies.txt") {
                        model.chooseCookiesFile()
                    }
                    .buttonStyle(CobaltButtonStyle(isPrimary: false))
                    .disabled(model.selectedMode == .galleryDl || model.state.isRunning)

                    Text("Export cookies in Netscape format from a browser session you own. Dawnload passes the file locally to yt-dlp.")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }

                DrawerCard {
                    Label("HTTP 403 recovery", systemImage: "shield.lefthalf.filled")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    Text("1. Start with Chrome 146.\n2. Try Chrome 145, Chrome 142, then Safari 18.4.\n3. If all are blocked, select cookies.txt above.\n4. Use the clean page URL, without search words appended.")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }

                DrawerCard {
                    Text("appearance")
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    DrawerToggle(title: "wallpaper glass", systemImage: "rectangle.on.rectangle", isOn: $model.useGlassBackground, disabled: false)

                    Text("Makes the app background nearly transparent so the desktop shows through.")
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.bottom, 4)
        }
    }
}

private struct ActivityDrawer: View {
    @ObservedObject var model: DownloadViewModel

    var body: some View {
        LogView(lines: model.logLines, commandPreview: model.commandPreview) {
            model.clearLog()
        }
    }
}

private struct AboutDrawer: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            DrawerCard {
                Text("dawnload")
                    .font(.system(size: 20, weight: .semibold, design: .monospaced))

                Text("A small native macOS shell around yt-dlp, gallery-dl, and ffmpeg.")
                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                    .foregroundStyle(DawnloadColors.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }

            EmptyState(
                systemImage: "lock.open",
                title: "local-first by design",
                detail: "Dawnload runs local command-line tools and writes files to the folder you choose."
            )
        }
    }
}

// MARK: - Components

private struct DawnloadSigil: View {
    let state: RunState
    let progress: Double?

    var body: some View {
        ZStack {
            Circle()
                .fill(
                    RadialGradient(
                        colors: [tone.opacity(0.28), DawnloadColors.glassTint, Color.clear],
                        center: .center,
                        startRadius: 10,
                        endRadius: 72
                    )
                )
                .frame(width: 136, height: 136)

            Circle()
                .fill(DawnloadColors.glassTint)
                .background(.ultraThinMaterial, in: Circle())
                .frame(width: 104, height: 104)
                .overlay(
                    Circle()
                        .stroke(
                            LinearGradient(
                                colors: [Color.white.opacity(0.45), tone.opacity(0.28), Color.white.opacity(0.08)],
                                startPoint: .topLeading,
                                endPoint: .bottomTrailing
                            ),
                            lineWidth: 1.4
                        )
                )
                .shadow(color: tone.opacity(0.24), radius: 24)

            VStack(spacing: 3) {
                Image(systemName: symbolName)
                    .font(.system(size: symbolSize, weight: .semibold))
                    .symbolRenderingMode(.hierarchical)
                    .foregroundStyle(symbolTone)

                if showsHorizon {
                    Capsule()
                        .fill(DawnloadColors.text.opacity(0.82))
                        .frame(width: 42, height: 3)
                }
            }

            if state.isRunning {
                Circle()
                    .trim(from: 0, to: CGFloat(progressFraction))
                    .stroke(tone, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .frame(width: 124, height: 124)
                    .rotationEffect(.degrees(-90))
                    .animation(.easeOut(duration: 0.18), value: progressFraction)
            }
        }
        .shadow(color: tone.opacity(0.2), radius: 24)
    }

    private var progressFraction: Double {
        guard let progress else { return 0 }
        return min(max(progress, 0), 1)
    }

    private var symbolName: String {
        switch state {
        case .failed:
            return "exclamationmark.triangle"
        case .completed:
            return "checkmark"
        case .cancelled:
            return "pause"
        case .queued:
            return "tray.full"
        case .running:
            return "arrow.down.circle.fill"
        case .idle:
            return "sun.max.fill"
        }
    }

    private var symbolSize: CGFloat {
        switch state {
        case .idle:
            return 39
        default:
            return 42
        }
    }

    private var symbolTone: Color {
        switch state {
        case .idle:
            return DawnloadColors.orange
        default:
            return DawnloadColors.text
        }
    }

    private var showsHorizon: Bool {
        if case .idle = state {
            return true
        }

        return false
    }

    private var tone: Color {
        switch state {
        case .failed:
            return DawnloadColors.red
        case .completed:
            return DawnloadColors.green
        case .queued, .cancelled:
            return DawnloadColors.orange
        default:
            return DawnloadColors.blue
        }
    }
}

private struct DockButton: View {
    let title: String
    let systemImage: String
    var badge: String?
    let isActive: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 3) {
                ZStack(alignment: .topTrailing) {
                    Image(systemName: systemImage)
                        .font(.system(size: 17, weight: .semibold))
                        .frame(height: 20)

                    if let badge {
                        Text(badge)
                            .font(.system(size: 9, weight: .bold, design: .monospaced))
                            .foregroundStyle(isActive ? DawnloadColors.text : DawnloadColors.stage)
                            .frame(minWidth: 16, minHeight: 16)
                            .background(Circle().fill(isActive ? DawnloadColors.stage : DawnloadColors.text))
                            .offset(x: 11, y: -7)
                    }
                }

                Text(title)
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
            }
            .frame(width: 64, height: 48)
            .foregroundStyle(isActive ? DawnloadColors.stage : DawnloadColors.text.opacity(0.74))
            .background(
                Capsule()
                    .fill(isActive ? DawnloadColors.text.opacity(0.94) : Color.clear)
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
    }
}

private enum ModeChipKind {
    case auto
    case video
    case audio
    case gallery

    var title: String {
        switch self {
        case .auto:
            return "auto"
        case .video:
            return "video"
        case .audio:
            return "audio"
        case .gallery:
            return "gallery"
        }
    }

    var systemImage: String {
        switch self {
        case .auto:
            return "sparkles"
        case .video:
            return "play.rectangle"
        case .audio:
            return "music.note"
        case .gallery:
            return "photo.on.rectangle"
        }
    }

    var color: Color {
        switch self {
        case .auto:
            return DawnloadColors.orange
        case .video:
            return DawnloadColors.blue
        case .audio:
            return DawnloadColors.purple
        case .gallery:
            return DawnloadColors.green
        }
    }
}

private struct BackendToggleButton: View {
    let title: String
    let detail: String
    let systemImage: String
    let isActive: Bool
    let disabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 9) {
                Image(systemName: systemImage)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(isActive ? DawnloadColors.stage : DawnloadColors.muted)
                    .frame(width: 20)

                VStack(alignment: .leading, spacing: 1) {
                    Text(title)
                        .font(.system(size: 14, weight: .semibold, design: .monospaced))

                    Text(detail)
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(isActive ? DawnloadColors.stage.opacity(0.72) : DawnloadColors.muted)
                }

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity)
            .frame(height: 42)
            .foregroundStyle(isActive ? DawnloadColors.stage : DawnloadColors.text)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(isActive ? DawnloadColors.text : Color.clear)
            )
            .opacity(disabled ? 0.45 : 1)
            .contentShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }
}

private struct ModeButton: View {
    let kind: ModeChipKind
    let isActive: Bool
    let disabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                Image(systemName: kind.systemImage)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(isActive ? kind.color : DawnloadColors.muted)

                Text(kind.title)
                    .font(.system(size: 14, weight: .semibold, design: .monospaced))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .padding(.horizontal, 14)
            .frame(height: 36)
            .foregroundStyle(isActive ? DawnloadColors.stage : DawnloadColors.text)
            .background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(isActive ? DawnloadColors.text : Color.clear)
            )
            .opacity(disabled ? 0.45 : 1)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }
}

private struct TogglePill: View {
    let title: String
    let systemImage: String
    @Binding var isOn: Bool
    let disabled: Bool

    var body: some View {
        Button {
            guard !disabled else { return }
            isOn.toggle()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: systemImage)
                    .font(.system(size: 12, weight: .semibold))

                Text(title)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
            }
            .padding(.horizontal, 10)
            .frame(height: 30)
            .foregroundStyle(isOn ? DawnloadColors.stage : DawnloadColors.muted)
            .background(
                Capsule()
                    .fill(isOn ? DawnloadColors.text : DawnloadColors.control)
            )
            .overlay(
                Capsule()
                    .stroke(DawnloadColors.hairline)
            )
            .opacity(disabled ? 0.4 : 1)
        }
        .buttonStyle(.plain)
        .disabled(disabled)
    }
}

private struct QueueStatusButton: View {
    let state: RunState
    let progress: Double?
    let queuedCount: Int
    let isActive: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack(alignment: .topTrailing) {
                ZStack {
                    Circle()
                        .fill(DawnloadColors.control)
                        .frame(width: 48, height: 48)

                    Circle()
                        .stroke(DawnloadColors.inputBorder, lineWidth: 2)
                        .frame(width: 48, height: 48)

                    if state.isRunning || progress != nil {
                        Circle()
                            .trim(from: 0, to: CGFloat(progress ?? 0.28))
                            .stroke(tone, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                            .rotationEffect(.degrees(-90))
                            .frame(width: 48, height: 48)
                    }

                    Image(systemName: icon)
                        .font(.system(size: 18, weight: .semibold))
                        .foregroundStyle(tone)
                }
                .shadow(color: .black.opacity(0.25), radius: 10, y: 3)

                if queuedCount > 0 {
                    Text("\(queuedCount)")
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(DawnloadColors.stage)
                        .frame(minWidth: 17, minHeight: 17)
                        .background(Circle().fill(DawnloadColors.text))
                        .offset(x: 3, y: -2)
                }
            }
        }
        .buttonStyle(.plain)
        .scaleEffect(isActive ? 1.04 : 1)
    }

    private var icon: String {
        switch state {
        case .completed:
            return "checkmark"
        case .failed:
            return "exclamationmark"
        case .cancelled:
            return "pause"
        default:
            return "tray.full"
        }
    }

    private var tone: Color {
        switch state {
        case .failed:
            return DawnloadColors.red
        case .completed:
            return DawnloadColors.green
        case .queued, .cancelled:
            return DawnloadColors.orange
        default:
            return DawnloadColors.text
        }
    }
}

private struct ProgressLine: View {
    let progress: Double?
    let tone: Color

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                Rectangle().fill(DawnloadColors.control)

                if let progress {
                    Rectangle()
                        .fill(tone)
                        .frame(width: max(2, proxy.size.width * progress))
                } else {
                    Rectangle()
                        .fill(
                            LinearGradient(
                                colors: [.clear, tone, .clear],
                                startPoint: .leading,
                                endPoint: .trailing
                            )
                        )
                        .frame(width: proxy.size.width * 0.32)
                        .offset(x: proxy.size.width * 0.34)
                }
            }
        }
    }
}

private struct DrawerCard<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            content
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: 15, style: .continuous)
                .fill(DawnloadColors.control)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 15, style: .continuous)
                .stroke(DawnloadColors.hairline)
        )
    }
}

private struct EmptyState: View {
    let systemImage: String
    let title: String
    let detail: String

    var body: some View {
        DrawerCard {
            HStack(alignment: .top, spacing: 12) {
                Image(systemName: systemImage)
                    .font(.system(size: 19, weight: .semibold))
                    .foregroundStyle(DawnloadColors.muted)
                    .frame(width: 28)

                VStack(alignment: .leading, spacing: 4) {
                    Text(title)
                        .font(.system(size: 13, weight: .semibold, design: .monospaced))

                    Text(detail)
                        .font(.system(size: 12, weight: .medium, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

private struct ToolRow: View {
    let tool: ToolStatus

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: tool.isInstalled ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                .foregroundStyle(tool.isInstalled ? DawnloadColors.green : DawnloadColors.orange)
                .font(.system(size: 16, weight: .semibold))

            VStack(alignment: .leading, spacing: 2) {
                Text(tool.name)
                    .font(.system(size: 13, weight: .semibold, design: .monospaced))

                Text(tool.path ?? "not found")
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundStyle(DawnloadColors.muted)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Spacer(minLength: 0)
        }
        .padding(.vertical, 2)
    }
}

private struct QueueRow: View {
    let item: DownloadQueueItem
    let index: Int
    let isActive: Bool
    let canRemove: Bool
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 11) {
            ZStack {
                Circle()
                    .fill(isActive ? DawnloadColors.blue.opacity(0.22) : DawnloadColors.control)
                    .frame(width: 29, height: 29)

                if isActive {
                    ProgressView()
                        .controlSize(.small)
                        .scaleEffect(0.62)
                } else {
                    Text("\(index)")
                        .font(.system(size: 11, weight: .bold, design: .monospaced))
                        .foregroundStyle(DawnloadColors.muted)
                }
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(item.displayURL)
                    .font(.system(size: 13, weight: .semibold, design: .monospaced))
                    .lineLimit(1)
                    .truncationMode(.middle)

                Text(item.settingsSummary)
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .foregroundStyle(DawnloadColors.muted)
                    .lineLimit(1)
            }

            Spacer(minLength: 6)

            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(DawnloadColors.muted)
                    .frame(width: 24, height: 24)
                    .background(Circle().fill(DawnloadColors.control))
            }
            .buttonStyle(.plain)
            .disabled(!canRemove)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .fill(isActive ? DawnloadColors.blue.opacity(0.09) : DawnloadColors.control)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 13, style: .continuous)
                .stroke(isActive ? DawnloadColors.blue.opacity(0.42) : DawnloadColors.hairline)
        )
    }
}

private struct DrawerToggle: View {
    let title: String
    let systemImage: String
    @Binding var isOn: Bool
    let disabled: Bool

    var body: some View {
        Toggle(isOn: $isOn) {
            Label {
                Text(title)
                    .font(.system(size: 13, weight: .medium, design: .monospaced))
            } icon: {
                Image(systemName: systemImage)
                    .foregroundStyle(DawnloadColors.muted)
            }
        }
        .toggleStyle(.switch)
        .controlSize(.small)
        .tint(DawnloadColors.blue)
        .disabled(disabled)
    }
}

private struct LogView: View {
    let lines: [String]
    let commandPreview: String
    let onClear: () -> Void

    var body: some View {
        DrawerCard {
            HStack {
                Text("terminal output")
                    .font(.system(size: 13, weight: .semibold, design: .monospaced))

                Spacer()

                Button("clear", action: onClear)
                    .buttonStyle(CobaltButtonStyle(isPrimary: false))
                    .disabled(lines.isEmpty)
            }

            ScrollViewReader { proxy in
                ScrollView {
                    VStack(alignment: .leading, spacing: 5) {
                        if lines.isEmpty {
                            Text("Downloader output will appear here.")
                                .foregroundStyle(DawnloadColors.muted)
                                .padding(.vertical, 8)
                        } else {
                            ForEach(Array(lines.enumerated()), id: \.offset) { index, line in
                                Text(line)
                                    .foregroundStyle(index == 0 && !commandPreview.isEmpty ? DawnloadColors.muted : DawnloadColors.text)
                                    .id(index)
                            }
                        }
                    }
                    .font(.system(size: 11, weight: .medium, design: .monospaced))
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(10)
                }
                .frame(maxHeight: .infinity)
                .background(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(DawnloadColors.stage)
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .stroke(DawnloadColors.hairline)
                )
                .onChange(of: lines.count) { count in
                    guard count > 0 else { return }
                    withAnimation(.easeOut(duration: 0.2)) {
                        proxy.scrollTo(count - 1, anchor: .bottom)
                    }
                }
            }
        }
    }
}

private struct WindowBackgroundConfigurator: NSViewRepresentable {
    let isGlassEnabled: Bool

    func makeNSView(context: Context) -> NSView {
        NSView()
    }

    func updateNSView(_ view: NSView, context: Context) {
        DispatchQueue.main.async {
            guard let window = view.window else { return }

            window.isOpaque = !isGlassEnabled
            window.backgroundColor = isGlassEnabled ? .clear : .black
            window.titlebarAppearsTransparent = true
            window.hasShadow = true
        }
    }
}

// MARK: - URL field

private struct NativeURLTextField: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let isDisabled: Bool
    @Binding var isEditing: Bool
    let focusTrigger: Int
    var onSubmit: () -> Void = {}

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text, isEditing: $isEditing, onSubmit: onSubmit)
    }

    func makeNSView(context: Context) -> NSTextField {
        let textField = NSTextField()
        textField.delegate = context.coordinator
        textField.isBordered = false
        textField.isBezeled = false
        textField.drawsBackground = false
        textField.backgroundColor = .clear
        textField.focusRingType = .none
        textField.placeholderString = placeholder
        textField.font = .monospacedSystemFont(ofSize: 15, weight: .medium)
        textField.textColor = NSColor.labelColor
        textField.lineBreakMode = .byTruncatingMiddle
        textField.cell?.usesSingleLineMode = true
        textField.cell?.wraps = false
        textField.cell?.isScrollable = true
        textField.setContentHuggingPriority(.defaultLow, for: .horizontal)
        textField.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        return textField
    }

    func updateNSView(_ textField: NSTextField, context: Context) {
        context.coordinator.onSubmit = onSubmit

        if textField.stringValue != text {
            textField.stringValue = text
        }

        textField.isEnabled = !isDisabled
        textField.textColor = isDisabled ? NSColor.disabledControlTextColor : NSColor.labelColor
        textField.placeholderString = placeholder

        if context.coordinator.lastFocusTrigger != focusTrigger {
            context.coordinator.lastFocusTrigger = focusTrigger
            if !isDisabled {
                DispatchQueue.main.async {
                    textField.window?.makeFirstResponder(textField)
                }
            }
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        @Binding private var text: String
        @Binding private var isEditing: Bool
        var onSubmit: () -> Void
        var lastFocusTrigger = 0

        init(text: Binding<String>, isEditing: Binding<Bool>, onSubmit: @escaping () -> Void) {
            _text = text
            _isEditing = isEditing
            self.onSubmit = onSubmit
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let textField = notification.object as? NSTextField else { return }
            text = textField.stringValue
        }

        func controlTextDidBeginEditing(_ notification: Notification) {
            DispatchQueue.main.async { self.isEditing = true }
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            DispatchQueue.main.async { self.isEditing = false }
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            if selector == #selector(NSResponder.insertNewline(_:)) {
                onSubmit()
                return true
            }
            return false
        }
    }
}

// MARK: - Styles

private struct CobaltButtonStyle: ButtonStyle {
    var isPrimary = true

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .lineLimit(1)
            .padding(.horizontal, 12)
            .frame(height: 32)
            .foregroundStyle(isPrimary ? DawnloadColors.stage : DawnloadColors.text)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(isPrimary ? DawnloadColors.text : DawnloadColors.controlRaised)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .stroke(DawnloadColors.hairline)
            )
            .opacity(configuration.isPressed ? 0.75 : 1)
            .scaleEffect(configuration.isPressed ? 0.98 : 1)
    }
}

private struct CobaltTextButtonStyle: ButtonStyle {
    var tone: Color = DawnloadColors.blue

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold, design: .monospaced))
            .foregroundStyle(tone)
            .opacity(configuration.isPressed ? 0.65 : 1)
    }
}

private enum DawnloadColors {
    static let stage = Color(red: 0.0, green: 0.0, blue: 0.0)
    static let rail = Color(red: 0.075, green: 0.075, blue: 0.075)
    static let drawer = Color(red: 0.095, green: 0.095, blue: 0.095)
    static let control = Color(red: 0.145, green: 0.145, blue: 0.145)
    static let controlRaised = Color(red: 0.175, green: 0.175, blue: 0.175)
    static let glassTint = Color.white.opacity(0.075)
    static let glassStroke = Color.white.opacity(0.16)
    static let inputBorder = Color(red: 0.27, green: 0.27, blue: 0.27)
    static let hairline = Color.white.opacity(0.07)
    static let text = Color(red: 0.88, green: 0.88, blue: 0.88)
    static let muted = Color(red: 0.55, green: 0.55, blue: 0.55)
    static let dim = Color(red: 0.36, green: 0.36, blue: 0.36)
    static let blue = Color(red: 0.18, green: 0.54, blue: 0.98)
    static let green = Color(red: 0.22, green: 0.67, blue: 0.26)
    static let orange = Color(red: 0.95, green: 0.6, blue: 0.22)
    static let red = Color(red: 0.93, green: 0.13, blue: 0.21)
    static let purple = Color(red: 0.48, green: 0.25, blue: 0.65)
}
