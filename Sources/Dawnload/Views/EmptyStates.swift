import SwiftUI

/// The empty list: one invitation, or the setup card when yt-dlp is missing.
struct EmptyHero: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            Spacer(minLength: 24)
            if model.ytDlpMissing {
                SetupCard(missing: model.missingTools)
                    .frame(maxWidth: 520)
            } else {
                invitation
            }
            Spacer(minLength: 24)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.horizontal, DLSpace.gutter)
    }

    private var invitation: some View {
        VStack(spacing: 18) {
            ZStack {
                CutDGlyph(cut: .arrow, style: .solid, color: DLColor.track)
            }
            .frame(width: 74, height: 96)
            .overlay {
                // The arrow in Signal, the D in track: the mark waiting for its first fill.
                ArrowOnly()
                    .frame(width: 74, height: 96)
            }
            .accessibilityHidden(true)

            Text("Paste a link to start.")
                .font(DLFont.display(26))
                .tracking(DLFont.displayTracking(26))
                .foregroundStyle(DLColor.text)
                .multilineTextAlignment(.center)

            Text("Video, audio and galleries from the web, kept on your Mac.")
                .font(.system(size: 13))
                .foregroundStyle(DLColor.text2)
                .multilineTextAlignment(.center)

            HStack(spacing: 20) {
                ShortcutHint(keys: "⇧⌘V", label: "Paste and Download")
                ShortcutHint(keys: "⌥↩", label: "Add to Queue")
            }
            .padding(.top, 6)
        }
    }
}

/// The arrow of the Cut D on its own, for the empty state.
private struct ArrowOnly: View {
    var body: some View {
        Canvas { context, size in
            let geo = CutDGeometry(fitting: size)
            let cut = geo.cut(.arrow)
            for stroke in cut.strokes {
                context.stroke(stroke, with: .color(DLColor.progress),
                               style: StrokeStyle(lineWidth: cut.weight, lineCap: .round, lineJoin: .round))
            }
        }
    }
}

/// Shown when a tool Dawnload needs isn't on this Mac.
struct SetupCard: View {
    @EnvironmentObject private var model: AppModel
    var missing: [ToolID]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                CutDGlyph(cut: .bang, style: .solid, color: DLColor.failed)
                    .frame(width: 22, height: 28)
                    .accessibilityLabel("Failed")
                Text(headline)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(DLColor.text)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Text("Dawnload runs yt-dlp, gallery-dl and ffmpeg from this Mac. Install them with Homebrew, then check again.")
                .font(.system(size: 13))
                .foregroundStyle(DLColor.text2)
                .fixedSize(horizontal: false, vertical: true)

            CommandBox(command: "brew install " + missing.map(\.brewFormula).joined(separator: " "))

            VStack(spacing: 8) {
                ForEach(model.tools) { tool in
                    ToolLine(status: tool)
                }
            }

            HStack {
                Button("Check Again") { model.refreshTools() }
                    .buttonStyle(SecondaryPillStyle(tone: DLColor.link))
                    .keyboardShortcut("r", modifiers: .command)
                Spacer()
                Link("Get Homebrew", destination: URL(string: "https://brew.sh")!)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(DLColor.link)
            }
        }
        .padding(DLSpace.xl)
        .dlSurface(radius: DLRadius.card)
    }

    private var headline: String {
        if missing.contains(.ytDlp) { return "Dawnload needs yt-dlp to download from the web." }
        let names = missing.map(\.rawValue).joined(separator: " and ")
        return "Dawnload needs \(names) for some downloads."
    }
}

/// A tool with its glyph, name, version and path.
struct ToolLine: View {
    var status: ToolStatus

    var body: some View {
        HStack(spacing: 10) {
            CutDGlyph(cut: status.isInstalled ? .check : .bang, style: .solid,
                      color: status.isInstalled ? DLColor.kept : DLColor.failed)
                .frame(width: 13, height: 16)
            Text(status.tool.rawValue)
                .font(DLFont.mono(12, bold: true))
                .foregroundStyle(DLColor.text)
            if let version = status.version {
                Text(version)
                    .font(DLFont.mono(11))
                    .foregroundStyle(DLColor.text2)
            }
            Spacer(minLength: 8)
            Text(status.path ?? "Not found")
                .font(DLFont.mono(11))
                .foregroundStyle(status.isInstalled ? DLColor.text2 : DLColor.failed)
                .lineLimit(1)
                .truncationMode(.middle)
                .textSelection(.enabled)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(status.tool.rawValue), \(status.isInstalled ? "found at \(status.path ?? "")" : "not found")")
    }
}
