import SwiftUI

/// Once, on first launch: what Dawnload is, what it found, where files go.
struct FirstRunView: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                DawnloadMark()
                    .frame(width: 34, height: 44)
                Wordmark()
                    .fill(DLColor.text)
                    .frame(width: 150, height: 23)
                    .accessibilityLabel("Dawnload")
            }
            .padding(.bottom, 28)

            Text("Keep what you find.")
                .font(DLFont.display(30))
                .tracking(DLFont.displayTracking(30))
                .foregroundStyle(DLColor.text)
                .padding(.bottom, 10)

            Text("Paste a link to a video, a playlist or a gallery, and Dawnload keeps the file on your Mac.")
                .font(.system(size: 14))
                .foregroundStyle(DLColor.text2)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.bottom, 26)

            VStack(alignment: .leading, spacing: 22) {
                step(number: "1", title: "Tools on this Mac") {
                    VStack(spacing: 8) {
                        ForEach(model.tools) { ToolLine(status: $0) }
                    }
                    if !model.missingTools.isEmpty {
                        CommandBox(command: "brew install " + model.missingTools.map(\.brewFormula).joined(separator: " "))
                            .padding(.top, 4)
                    }
                }

                step(number: "2", title: "Where files go") {
                    HStack {
                        Text(model.draft.destination.path)
                            .font(DLFont.mono(12))
                            .foregroundStyle(DLColor.text)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        Spacer()
                        Button("Change…") { model.chooseDestination() }
                            .buttonStyle(LinkButtonStyle())
                    }
                }

                step(number: "3", title: "Yours to keep") {
                    Text("Dawnload is for media you have the right to keep. It runs yt-dlp, gallery-dl and ffmpeg on this Mac; nothing goes through a server.")
                        .font(.system(size: 12))
                        .foregroundStyle(DLColor.text2)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }

            HStack {
                if !model.missingTools.isEmpty {
                    Button("Check Again") { model.refreshTools() }
                        .buttonStyle(SecondaryPillStyle())
                }
                Spacer()
                Button {
                    model.completeFirstRun()
                } label: {
                    Text("Start")
                        .frame(minWidth: 80)
                }
                .buttonStyle(ActionPillStyle(height: 36))
                .keyboardShortcut(.defaultAction)
            }
            .padding(.top, 30)
        }
        .padding(36)
        .frame(width: 560)
        .background(DLColor.canvas)
    }

    private func step<Content: View>(number: String, title: String, @ViewBuilder content: () -> Content) -> some View {
        HStack(alignment: .top, spacing: 14) {
            Text(number)
                .font(DLFont.mono(11, bold: true))
                .foregroundStyle(DLColor.link)
                .frame(width: 22, height: 22)
                .overlay(Circle().strokeBorder(DLColor.rule))
            VStack(alignment: .leading, spacing: 10) {
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(DLColor.text)
                content()
            }
        }
    }
}
