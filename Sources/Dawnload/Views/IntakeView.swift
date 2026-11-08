import AppKit
import DawnloadCore
import SwiftUI

/// The paste field, the detection line and the option chips.
struct IntakeView: View {
    @EnvironmentObject private var model: AppModel
    @State private var isEditing = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            field
            detectionLine
            chips
        }
    }

    // MARK: Field

    private var field: some View {
        HStack(spacing: 0) {
            Image(systemName: kindSymbol)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(isEditing ? DLColor.link : DLColor.text2)
                .frame(width: 48)
                .accessibilityHidden(true)

            URLField(
                text: $model.draftText,
                placeholder: "Paste a link to a video, a playlist or a gallery",
                focusTrigger: model.focusIntakeTrigger,
                isEditing: $isEditing,
                onSubmit: { model.download() },
                onAlternateSubmit: { model.addToQueue() }
            )
            .frame(height: 56)
            .accessibilityLabel("Link")

            if !model.draftText.isEmpty {
                Button {
                    model.draftText = ""
                    model.focusIntakeTrigger += 1
                } label: {
                    Image(systemName: "xmark")
                }
                .buttonStyle(RoundIconButtonStyle(size: 22))
                .help("Clear the field")
                .accessibilityLabel("Clear")
                .padding(.trailing, 8)
            }

            Button {
                model.download()
            } label: {
                HStack(spacing: 6) {
                    PullArrow()
                    Text("Download")
                }
            }
            .buttonStyle(ActionPillStyle())
            .disabled(!model.canSubmit)
            .help("Download (Return). Add to Queue with ⌥Return.")
            .padding(.trailing, 10)
        }
        .background(RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous).fill(DLColor.raised))
        .overlay(
            RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous)
                .strokeBorder(isEditing ? DLColor.link : DLColor.rule, lineWidth: isEditing ? 2 : 1)
        )
        .overlay(alignment: .leading) {
            Spine(isVisible: isEditing)
                .padding(.vertical, 12)
                .offset(x: -1.5)
        }
        .animation(DLMotion.swap, value: isEditing)
    }

    private var kindSymbol: String {
        switch model.detectedSource?.kind {
        case .magnet: "link.badge.plus"
        case .torrentFile: "doc"
        default: "link"
        }
    }

    // MARK: Detection

    @ViewBuilder
    private var detectionLine: some View {
        let detection = model.detection
        HStack(spacing: 6) {
            if let detection {
                Image(systemName: symbol(for: detection.tone))
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(color(for: detection.tone))
                Text(detection.text)
                    .font(DLFont.mono(11))
                    .foregroundStyle(detection.tone == .neutral ? DLColor.text2 : color(for: detection.tone))
                    .lineLimit(2)
            } else {
                Text("Return downloads now. ⌥Return adds to the queue.")
                    .font(.system(size: 11))
                    .foregroundStyle(DLColor.text2.opacity(0.75))
            }
        }
        .padding(.leading, 4)
        .frame(minHeight: 16, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private func symbol(for tone: Detection.Tone) -> String {
        switch tone {
        case .neutral: "arrow.down.circle"
        case .warning: "exclamationmark.triangle"
        case .failed: "exclamationmark.circle"
        }
    }

    private func color(for tone: Detection.Tone) -> Color {
        switch tone {
        case .neutral: DLColor.link
        case .warning: DLColor.text
        case .failed: DLColor.failed
        }
    }

    // MARK: Chips

    private var chips: some View {
        let engine = model.resolvedEngine
        let capabilities = EngineRegistry.engine(engine).capabilities

        return HStack(spacing: 6) {
            MenuChip(title: formatTitle(engine), help: "Format") {
                QualityPreference.allCases.map { quality in
                    MenuChip.Entry(title: quality.title, subtitle: quality.subtitle,
                                   isChecked: model.draft.quality == quality) {
                        model.draft.quality = quality
                    }
                }
            }
            .disabled(!capabilities.contains(.formats))

            MenuChip(title: engineTitle(engine), help: "Engine") {
                [
                    MenuChip.Entry(title: "Auto", subtitle: "gallery-dl for gallery sites, yt-dlp for the rest",
                                   isChecked: model.draft.engine == .auto) { model.draft.engine = .auto },
                    .separator,
                    MenuChip.Entry(title: "yt-dlp", subtitle: ToolID.ytDlp.purpose,
                                   isChecked: model.draft.engine == .ytDlp) { model.draft.engine = .ytDlp },
                    MenuChip.Entry(title: "gallery-dl", subtitle: ToolID.galleryDl.purpose,
                                   isChecked: model.draft.engine == .galleryDl) { model.draft.engine = .galleryDl }
                ]
            }

            ToggleChip(title: "Polish", isOn: $model.draft.polish,
                       help: "ffmpeg merges audio and video, remuxes to MP4 and moves the index to the front")
                .disabled(!capabilities.contains(.polish))

            ToggleChip(title: "Playlist", isOn: $model.draft.playlist,
                       help: "Download the whole playlist this link belongs to")
                .disabled(!capabilities.contains(.playlists))

            MenuChip(title: model.draft.impersonate ? model.draft.impersonationTarget.title : "Impersonate",
                     isOn: model.draft.impersonate, leadingSymbol: "person.crop.circle",
                     help: "Browser impersonation, for sites that refuse yt-dlp") {
                [MenuChip.Entry(title: "Off", isChecked: !model.draft.impersonate) { model.draft.impersonate = false }, .separator]
                    + BrowserImpersonationTarget.allCases.map { target in
                        MenuChip.Entry(title: target.title,
                                       isChecked: model.draft.impersonate && model.draft.impersonationTarget == target) {
                            model.draft.impersonate = true
                            model.draft.impersonationTarget = target
                        }
                    }
            }
            .disabled(!capabilities.contains(.siteAccess))

            if model.cookiesFile != nil {
                ToggleChip(title: "cookies.txt", isOn: $model.draft.useCookies,
                           help: "Send the cookies.txt from Settings with this download")
                    .disabled(!capabilities.contains(.siteAccess))
            }

            Spacer(minLength: 8)

            MenuChip(title: model.draft.destination.lastPathComponent, leadingSymbol: "folder",
                     help: "Download folder: \(model.draft.destination.path)") {
                [
                    MenuChip.Entry(title: "Choose Folder…") { model.chooseDestination() },
                    MenuChip.Entry(title: "Show in Finder") { model.revealDestination() }
                ]
            }
        }
    }

    private func formatTitle(_ engine: EngineID) -> String {
        switch engine {
        case .galleryDl: "Original files"
        case .torrent: "All files"
        case .ytDlp: model.draft.quality.title
        }
    }

    private func engineTitle(_ engine: EngineID) -> String {
        switch model.draft.engine {
        case .auto: engine == .torrent ? "Auto · Torrent" : "Auto · \(engine.rawValue)"
        case .ytDlp: "yt-dlp"
        case .galleryDl: "gallery-dl"
        }
    }
}

/// An NSTextField in Martian Mono. Return submits, ⌥Return submits to the queue.
struct URLField: NSViewRepresentable {
    @Binding var text: String
    let placeholder: String
    let focusTrigger: Int
    @Binding var isEditing: Bool
    var onSubmit: () -> Void
    var onAlternateSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let field = FocusAwareTextField()
        field.onFocus = { [weak coordinator = context.coordinator] in
            DispatchQueue.main.async { coordinator?.parent.isEditing = true }
        }
        field.delegate = context.coordinator
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = DLFont.nsMono(13)
        field.textColor = DLColor.dynamic(dark: DLPalette.paper, light: DLPalette.pine)
        field.lineBreakMode = .byTruncatingMiddle
        field.cell?.usesSingleLineMode = true
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: [
            .font: DLFont.nsMono(13),
            .foregroundColor: DLColor.dynamic(dark: DLPalette.sage, light: DLPalette.lichen)
        ])
        DispatchQueue.main.async { field.window?.makeFirstResponder(field) }
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text {
            field.stringValue = text
        }
        if context.coordinator.lastFocusTrigger != focusTrigger {
            context.coordinator.lastFocusTrigger = focusTrigger
            DispatchQueue.main.async {
                field.window?.makeFirstResponder(field)
                field.currentEditor()?.selectedRange = NSRange(location: field.stringValue.count, length: 0)
            }
        }
    }

    /// Reports focus as soon as the field becomes first responder, not only once typing starts.
    final class FocusAwareTextField: NSTextField {
        var onFocus: (() -> Void)?

        override func becomeFirstResponder() -> Bool {
            let became = super.becomeFirstResponder()
            if became { onFocus?() }
            return became
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: URLField
        var lastFocusTrigger: Int

        init(_ parent: URLField) {
            self.parent = parent
            lastFocusTrigger = parent.focusTrigger
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.text = field.stringValue
        }

        func controlTextDidBeginEditing(_ notification: Notification) {
            DispatchQueue.main.async { self.parent.isEditing = true }
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            DispatchQueue.main.async { self.parent.isEditing = false }
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            switch selector {
            case #selector(NSResponder.insertNewline(_:)):
                if NSApp.currentEvent?.modifierFlags.contains(.option) == true {
                    parent.onAlternateSubmit()
                } else {
                    parent.onSubmit()
                }
                return true
            case #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)), #selector(NSResponder.insertLineBreak(_:)):
                parent.onAlternateSubmit()
                return true
            default:
                return false
            }
        }
    }
}
