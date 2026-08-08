import SwiftUI

@main
struct DawnloadApp: App {
    @StateObject private var model = DownloadViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(model)
                .frame(minWidth: 720, minHeight: 620)
        }
        .windowStyle(.hiddenTitleBar)
        .windowResizability(.contentMinSize)
        .defaultSize(width: 860, height: 720)
        .commands {
            CommandGroup(replacing: .newItem) {}

            CommandMenu("Download") {
                Button("Download") { model.startDownload() }
                    .keyboardShortcut("d", modifiers: .command)
                    .disabled(!model.canStart)

                Button("Add to Queue") { _ = model.addCurrentToQueue() }
                    .keyboardShortcut("a", modifiers: [.command, .shift])
                    .disabled(!model.canAddToQueue)

                Divider()

                Button("Start Queue") { model.startQueue() }
                    .keyboardShortcut("s", modifiers: [.command, .shift])
                    .disabled(!model.canStartQueue)

                Button("Choose Destination…") { model.chooseDestination() }
                    .keyboardShortcut("o", modifiers: [.command, .shift])
                    .disabled(model.state.isRunning)

                Button("Refresh Tools") { model.refreshTools() }
                    .keyboardShortcut("r", modifiers: .command)
            }
        }
    }
}
