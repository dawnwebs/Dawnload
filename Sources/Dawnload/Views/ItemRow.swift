import DawnloadCore
import SwiftUI

/// One download: the glyph, the title, the subline, and a hover action.
struct ItemRow: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var item: DownloadItem
    var isSelected: Bool
    @State private var hovering = false

    var body: some View {
        HStack(spacing: 12) {
            StatusGlyph(status: item.status, fill: item.fill, indeterminate: item.isIndeterminate)
                .frame(width: 26, height: 32)

            VStack(alignment: .leading, spacing: 3) {
                Text(item.displayTitle)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(DLColor.text)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(subline)
                    .font(DLFont.mono(11))
                    .foregroundStyle(item.status == .failed ? DLColor.failed : DLColor.text2)
                    .lineLimit(1)
                    .truncationMode(.tail)
            }

            Spacer(minLength: 8)

            trailing
        }
        .padding(.leading, 14)
        .padding(.trailing, 12)
        .frame(height: 60)
        .background(
            RoundedRectangle(cornerRadius: DLRadius.spine, style: .continuous)
                .fill(isSelected ? DLColor.raised : (hovering ? DLColor.raised.opacity(0.5) : Color.clear))
        )
        .overlay(alignment: .leading) {
            Spine(isVisible: isSelected)
                .padding(.vertical, 14)
        }
        .contentShape(Rectangle())
        .onHover { hovering = $0 }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilitySentence)
        .accessibilityValue(item.status == .pulling ? (item.transfer.fraction.map(DLFormat.percent) ?? "") : "")
        .accessibilityAction(named: primaryActionTitle) { model.primaryAction(for: item) }
        .accessibilityAction(named: "Remove") { model.remove([item.id]) }
    }

    // MARK: Trailing

    @ViewBuilder
    private var trailing: some View {
        if hovering, let action = hoverAction {
            Button(action: action.run) {
                Image(systemName: action.symbol)
            }
            .buttonStyle(RoundIconButtonStyle())
            .help(action.title)
            .accessibilityLabel(action.title)
            .transition(.opacity)
        } else if item.status == .pulling, let fraction = item.transfer.fraction {
            Text(DLFormat.percent(fraction))
                .font(DLFont.mono(12, bold: true))
                .foregroundStyle(DLColor.text)
                .monospacedDigit()
        } else if item.status == .queued, hovering {
            Image(systemName: "line.3.horizontal")
                .foregroundStyle(DLColor.text2)
        }
    }

    private struct HoverAction {
        var title: String
        var symbol: String
        var run: () -> Void
    }

    private var hoverAction: HoverAction? {
        switch item.status {
        case .kept: HoverAction(title: "Show in Finder", symbol: "magnifyingglass") { model.reveal(item) }
        case .failed: HoverAction(title: "Retry", symbol: "arrow.clockwise") { model.runSoon(item) }
        case .paused: HoverAction(title: "Resume", symbol: "play.fill") { model.runSoon(item) }
        case .pulling, .polishing: HoverAction(title: "Pause", symbol: "pause.fill") { model.pause(item) }
        case .seeding: HoverAction(title: "Stop Seeding", symbol: "stop.fill") { model.stopSeeding(item) }
        case .queued: nil
        }
    }

    private var primaryActionTitle: String {
        switch item.status {
        case .kept: "Open"
        case .seeding: "Stop Seeding"
        case .pulling, .polishing: "Pause"
        case .queued: "Download Now"
        case .paused: "Resume"
        case .failed: "Retry"
        }
    }

    // MARK: Words

    private var subline: String {
        ItemWords.subline(for: item, queuePosition: model.queuePosition(of: item))
    }

    private var accessibilitySentence: String {
        [item.displayTitle, item.status.word, subline].joined(separator: ", ")
    }
}

/// The sentences rows and the inspector use. Kept in one place so the voice stays the same.
enum ItemWords {
    @MainActor
    static func subline(for item: DownloadItem, queuePosition: Int?) -> String {
        let t = item.transfer
        switch item.status {
        case .queued:
            var parts = [queuePosition.map(DLFormat.queuePosition) ?? "Queued", item.source.shortName]
            if item.source.isTorrent, !item.files.isEmpty {
                parts.append("\(item.keptFiles.count) of \(DLFormat.count(item.files.count, "file"))")
            }
            let summary = item.options.summary(for: item.engine)
            if !summary.isEmpty { parts.append(summary) }
            return parts.joined(separator: " · ")

        case .pulling:
            var parts: [String] = []
            if let detail = item.phaseDetail { parts.append(detail) }
            if let index = t.entryIndex, let count = t.entryCount, count > 1 {
                parts.append("Video \(index) of \(count)")
            }
            if let fraction = t.fraction, t.entryCount == nil || t.entryCount == 1 {
                parts.append(DLFormat.percent(fraction))
            }
            if let done = t.bytesDone, let total = t.bytesTotal, t.entryCount ?? 1 <= 1 {
                parts.append(DLFormat.bytes(done, of: total))
            } else if let done = t.bytesDone, done > 0 {
                parts.append(DLFormat.bytes(done))
            }
            if let speed = t.bytesPerSecond, speed > 0 { parts.append(DLFormat.speed(speed)) }
            if let left = t.secondsLeft { parts.append(DLFormat.timeLeft(left)) }
            if let swarm = item.swarm { parts.append(DLFormat.count(swarm.peers, "peer")) }
            return parts.isEmpty ? "Pulling · \(item.source.shortName)" : parts.joined(separator: " · ")

        case .polishing:
            return "Polishing · " + (item.phaseDetail ?? "finishing with ffmpeg")

        case .kept:
            var parts = ["Kept in \(item.options.destination.lastPathComponent)"]
            let count = item.keptFiles.count
            if count > 0 { parts.append(DLFormat.count(count, "file")) }
            if let bytes = item.keptBytes, bytes > 0 { parts.append(DLFormat.bytes(bytes)) }
            return parts.joined(separator: " · ")

        case .failed:
            return item.failure?.message ?? "Failed."

        case .paused:
            if let fraction = t.fraction, fraction > 0 {
                var text = "Paused at \(DLFormat.percent(fraction))"
                if let index = t.entryIndex, let count = t.entryCount, count > 1 {
                    text += " · Video \(index) of \(count)"
                } else if let done = t.bytesDone, let total = t.bytesTotal {
                    text += " · " + DLFormat.bytes(done, of: total)
                }
                return text
            }
            return "Paused · Resume to continue"

        case .seeding:
            guard let swarm = item.swarm else { return "Seeding" }
            return [
                "Seeding",
                "↑ " + DLFormat.speed(swarm.uploadRate),
                "ratio " + DLFormat.ratio(swarm.ratio),
                DLFormat.count(swarm.peers, "peer")
            ].joined(separator: " · ")
        }
    }
}
