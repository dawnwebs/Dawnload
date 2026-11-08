import AppKit
import SwiftUI

// MARK: - Status glyph

/// The Cut D for an item's state. Glyph and word always travel together: the word is the label.
struct StatusGlyph: View {
    let status: ItemStatus
    var fill: Double = 0
    var indeterminate = false

    var body: some View {
        ZStack {
            glyph
                .id(GlyphKey(status: status, indeterminate: indeterminate))
                .transition(.opacity)
        }
        .animation(DLMotion.swap, value: GlyphKey(status: status, indeterminate: indeterminate))
        .accessibilityElement()
        .accessibilityLabel(status.word)
    }

    @ViewBuilder
    private var glyph: some View {
        switch status {
        case .queued:
            CutDGlyph(cut: .dots, style: .outline, color: DLColor.text2)
        case .pulling:
            if indeterminate {
                CutDGlyph(cut: .arrow, style: .loader, color: DLColor.progress)
            } else {
                CutDGlyph(cut: .arrow, style: .filling, color: DLColor.progress, fill: fill)
                    .animation(DLMotion.fill, value: fill)
            }
        case .polishing:
            CutDGlyph(cut: .merge, style: .solid, color: DLColor.progress)
        case .kept:
            CutDGlyph(cut: .check, style: .solid, color: DLColor.kept)
        case .failed:
            CutDGlyph(cut: .bang, style: .solid, color: DLColor.failed)
        case .paused:
            CutDGlyph(cut: .pause, style: .filling, color: DLColor.text2, fill: fill)
        case .seeding:
            CutDGlyph(cut: .arrowUp, style: .solid, color: DLColor.kept)
        }
    }

    private struct GlyphKey: Hashable {
        let status: ItemStatus
        let indeterminate: Bool
    }
}

extension DownloadItem {
    /// Pulling without a known size shows the loader instead of a fill.
    var isIndeterminate: Bool { status == .pulling && transfer.fraction == nil }

    var statusColor: Color {
        switch status {
        case .failed: DLColor.failed
        case .kept, .seeding: DLColor.kept
        case .queued, .paused: DLColor.text2
        case .pulling, .polishing: DLColor.progress
        }
    }
}

// MARK: - Buttons

/// The Download button: Signal pill, Pine label, the arrow presses down. One per view.
struct ActionPillStyle: ButtonStyle {
    var height: CGFloat = 36
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(DLColor.onAction)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 16)
            .frame(height: height)
            .background(Capsule().fill(DLColor.action))
            .opacity(isEnabled ? 1 : 0.4)
            .brightness(configuration.isPressed ? -0.06 : 0)
            .environment(\.dlPressed, configuration.isPressed)
            .contentShape(Capsule())
    }
}

/// Secondary actions: Start Queue, Retry, Show in Finder. Raised, text label, pill.
struct SecondaryPillStyle: ButtonStyle {
    var height: CGFloat = 28
    var tone: Color = DLColor.text
    @Environment(\.isEnabled) private var isEnabled
    @State private var hovering = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(tone)
            .lineLimit(1)
            .fixedSize()
            .padding(.horizontal, 12)
            .frame(height: height)
            .background(Capsule().fill(configuration.isPressed || hovering ? DLColor.raisedHover : DLColor.raised))
            .overlay(Capsule().strokeBorder(DLColor.rule, lineWidth: 1))
            .opacity(isEnabled ? 1 : 0.45)
            .contentShape(Capsule())
            .onHover { hovering = $0 }
    }
}

/// Icon-only round button for hover actions on rows.
struct RoundIconButtonStyle: ButtonStyle {
    var size: CGFloat = 26
    var tone: Color = DLColor.text2
    @State private var hovering = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(hovering ? DLColor.text : tone)
            .frame(width: size, height: size)
            .background(Circle().fill(configuration.isPressed || hovering ? DLColor.raisedHover : DLColor.raised))
            .overlay(Circle().strokeBorder(DLColor.rule, lineWidth: 1))
            .contentShape(Circle())
            .onHover { hovering = $0 }
    }
}

/// Text links in `link`.
struct LinkButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold))
            .foregroundStyle(DLColor.link)
            .opacity(configuration.isPressed ? 0.6 : 1)
            .contentShape(Rectangle())
    }
}

private struct PressedKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var dlPressed: Bool {
        get { self[PressedKey.self] }
        set { self[PressedKey.self] = newValue }
    }
}

/// The arrow inside the Download button. It presses down 3 pt and springs back (the pull).
struct PullArrow: View {
    @Environment(\.dlPressed) private var pressed
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Image(systemName: "arrow.down")
            .font(.system(size: 12, weight: .heavy))
            .offset(y: pressed && !reduceMotion ? 3 : 0)
            .animation(DLMotion.pull, value: pressed)
    }
}

// MARK: - Chips

/// A pill on the intake row. On: raised with a link-coloured check. Off: outline, quiet label.
struct ChipLabel: View {
    var title: String
    var isOn: Bool
    var showsChevron = false
    var leadingSymbol: String?
    @Environment(\.isEnabled) private var isEnabled

    var body: some View {
        HStack(spacing: 5) {
            if let leadingSymbol {
                Image(systemName: leadingSymbol)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(isOn ? DLColor.link : DLColor.text2)
            } else if isOn {
                Image(systemName: "checkmark")
                    .font(.system(size: 9, weight: .heavy))
                    .foregroundStyle(DLColor.link)
            }
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(isOn ? DLColor.text : DLColor.text2)
                .lineLimit(1)
            if showsChevron {
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(DLColor.text2)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 26)
        .background(Capsule().fill(isOn ? DLColor.raised : Color.clear))
        .overlay(Capsule().strokeBorder(DLColor.rule, lineWidth: 1))
        .opacity(isEnabled ? 1 : 0.45)
        .contentShape(Capsule())
    }
}

struct ToggleChip: View {
    var title: String
    @Binding var isOn: Bool
    var help: String

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            ChipLabel(title: title, isOn: isOn)
        }
        .buttonStyle(.plain)
        .help(help)
        .accessibilityLabel(title)
        .accessibilityValue(isOn ? "On" : "Off")
        .accessibilityAddTraits(.isButton)
    }
}

/// A chip that opens a native menu. Built on NSMenu so the chip keeps the brand's look.
struct MenuChip: View {
    struct Entry {
        var title: String
        var subtitle: String? = nil
        var isChecked = false
        var isEnabled = true
        var isSeparator = false
        var action: () -> Void = {}

        static let separator = Entry(title: "", isSeparator: true)
    }

    var title: String
    var isOn = true
    var leadingSymbol: String?
    var help: String
    var entries: () -> [Entry]

    var body: some View {
        MenuAnchor(entries: entries) {
            ChipLabel(title: title, isOn: isOn, showsChevron: true, leadingSymbol: leadingSymbol)
        }
        .help(help)
        .accessibilityLabel(help)
        .accessibilityValue(title)
    }
}

/// Shows an NSMenu under its content when clicked.
struct MenuAnchor<Label: View>: View {
    var entries: () -> [MenuChip.Entry]
    @ViewBuilder var label: () -> Label
    @State private var anchor = MenuAnchorView.Reference()

    var body: some View {
        Button {
            anchor.view?.popUp(entries())
        } label: {
            label()
        }
        .buttonStyle(.plain)
        .background(MenuAnchorView(reference: anchor))
    }
}

struct MenuAnchorView: NSViewRepresentable {
    final class Reference {
        weak var view: AnchorNSView?
    }

    let reference: Reference

    func makeNSView(context: Context) -> AnchorNSView {
        let view = AnchorNSView()
        reference.view = view
        return view
    }

    func updateNSView(_ nsView: AnchorNSView, context: Context) {
        reference.view = nsView
    }

    final class AnchorNSView: NSView {
        private var handlers: [() -> Void] = []

        func popUp(_ entries: [MenuChip.Entry]) {
            let menu = NSMenu()
            menu.autoenablesItems = false
            handlers = []
            for entry in entries {
                if entry.isSeparator {
                    menu.addItem(.separator())
                    continue
                }
                let item = NSMenuItem(title: entry.title, action: #selector(choose(_:)), keyEquivalent: "")
                item.target = self
                item.tag = handlers.count
                item.state = entry.isChecked ? .on : .off
                item.isEnabled = entry.isEnabled
                if let subtitle = entry.subtitle {
                    let text = NSMutableAttributedString(string: entry.title, attributes: [.font: NSFont.menuFont(ofSize: 13)])
                    text.append(NSAttributedString(string: "\n" + subtitle, attributes: [
                        .font: NSFont.menuFont(ofSize: 11),
                        .foregroundColor: NSColor.secondaryLabelColor
                    ]))
                    item.attributedTitle = text
                }
                handlers.append(entry.action)
                menu.addItem(item)
            }
            menu.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
        }

        override var isFlipped: Bool { true }

        @objc private func choose(_ sender: NSMenuItem) {
            guard handlers.indices.contains(sender.tag) else { return }
            handlers[sender.tag]()
        }
    }
}

// MARK: - Bits

/// The selection spine: a 3 pt bar on the leading edge, from the D's spine.
struct Spine: View {
    var isVisible: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
            .fill(DLColor.link)
            .frame(width: 3)
            .opacity(isVisible ? 1 : 0)
    }
}

/// Small uppercase-free section label in mono, for list sections and inspector parts.
struct SectionLabel: View {
    var title: String
    var trailing: String?

    var body: some View {
        HStack(spacing: 6) {
            Text(title)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(DLColor.text2)
            if let trailing {
                Text(trailing)
                    .font(DLFont.mono(10))
                    .foregroundStyle(DLColor.text2.opacity(0.8))
            }
        }
        .accessibilityAddTraits(.isHeader)
    }
}

/// A key–value row in the inspector: label in SF Pro, value in mono.
struct FactRow: View {
    var label: String
    var value: String
    var valueColor: Color = DLColor.text

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(DLColor.text2)
            Spacer(minLength: 12)
            Text(value)
                .font(DLFont.mono(11))
                .foregroundStyle(valueColor)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
        .accessibilityElement(children: .combine)
    }
}

/// A keyboard shortcut hint: "⇧⌘V Paste and Download".
struct ShortcutHint: View {
    var keys: String
    var label: String

    var body: some View {
        HStack(spacing: 8) {
            Text(keys)
                .font(DLFont.mono(11, bold: true))
                .foregroundStyle(DLColor.text)
                .padding(.horizontal, 7)
                .frame(height: 22)
                .background(RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous).fill(DLColor.raised))
                .overlay(RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous).strokeBorder(DLColor.rule))
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(DLColor.text2)
        }
        .accessibilityElement(children: .combine)
    }
}

/// A copyable command in mono, e.g. the Homebrew install line.
struct CommandBox: View {
    var command: String
    @State private var copied = false

    var body: some View {
        HStack(spacing: 10) {
            Text(command)
                .font(DLFont.mono(12))
                .foregroundStyle(DLColor.text)
                .textSelection(.enabled)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 8)
            Button(copied ? "Copied" : "Copy") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(command, forType: .string)
                copied = true
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.6) { copied = false }
            }
            .buttonStyle(LinkButtonStyle())
        }
        .padding(.horizontal, 12)
        .frame(height: 36)
        .background(RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous).fill(DLColor.canvas))
        .overlay(RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous).strokeBorder(DLColor.rule))
    }
}

extension View {
    /// Surfaces: raised fill with a hairline, at a radius.
    func dlSurface(radius: CGFloat = DLRadius.spine, fill: Color = DLColor.raised) -> some View {
        background(RoundedRectangle(cornerRadius: radius, style: .continuous).fill(fill))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(DLColor.rule, lineWidth: 1))
    }
}
