import AppKit
import SwiftUI

@main
struct DawnloadApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var model: AppModel

    init() {
        FontRegistry.registerBundledFonts()
        let model = AppModel()
        _model = StateObject(wrappedValue: model)
        AppDelegate.model = model
    }

    var body: some Scene {
        Window("Dawnload", id: "main") {
            MainWindow()
                .environmentObject(model)
                .frame(minWidth: 780, minHeight: 540)
                .preferredColorScheme(model.appearance.colorScheme)
        }
        .defaultSize(width: 1080, height: 720)
        .windowResizability(.contentMinSize)
        .windowToolbarStyle(.unified(showsTitle: false))
        .commands { DawnloadCommands(model: model) }

        Settings {
            SettingsView()
                .environmentObject(model)
        }
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    @MainActor static var model: AppModel?

    @MainActor
    func applicationDidFinishLaunching(_ notification: Notification) {
        Self.model?.downloadLaunchArguments()
        if Self.model?.isDemo == true, let raw = UserDefaults.standard.string(forKey: "DawnloadSettings"),
           let tab = SettingsTab(rawValue: raw) {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { SettingsOpener.open(tab: tab) }
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        true
    }

    @MainActor
    func applicationWillTerminate(_ notification: Notification) {
        Self.model?.prepareForQuit()
    }
}

struct DawnloadCommands: Commands {
    @ObservedObject var model: AppModel

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Open Torrent File…") { model.openTorrentFile() }
                .keyboardShortcut("o", modifiers: .command)
                .disabled(!model.torrentsEnabled)
        }

        CommandGroup(after: .pasteboard) {
            Divider()
            Button("Paste and Download") { model.pasteAndDownload() }
                .keyboardShortcut("v", modifiers: [.command, .shift])
        }

        CommandMenu("Download") {
            Button("Download") { model.download() }
                .keyboardShortcut("d", modifiers: .command)
                .disabled(!model.canSubmit)
            Button("Add to Queue") { model.addToQueue() }
                .keyboardShortcut("a", modifiers: [.command, .shift])
                .disabled(!model.canSubmit)
            Button("Focus Link Field") { model.focusIntakeTrigger += 1 }
                .keyboardShortcut("l", modifiers: .command)

            Divider()

            Button("Start Queue") { model.startQueue() }
                .keyboardShortcut("s", modifiers: [.command, .shift])
                .disabled(!model.hasWaiting)
            Button("Pause All") { model.pauseAll() }
                .keyboardShortcut(".", modifiers: .command)
                .disabled(!model.hasActive)
            Button("Resume All") { model.resumeAll() }
                .disabled(!model.hasPaused)

            Divider()

            let selected = model.selectedItem
            Button(primaryTitle(selected)) {
                if let selected { model.primaryAction(for: selected) }
            }
            .keyboardShortcut(.return, modifiers: .command)
            .disabled(selected == nil)
            Button("Show in Finder") {
                if let selected { model.reveal(selected) }
            }
            .keyboardShortcut("r", modifiers: [.command, .shift])
            .disabled(selected == nil)
            Button("Copy Link") {
                if let selected { model.copyLink(selected) }
            }
            .disabled(selected == nil)
            Button("Remove") {
                if let selected { model.remove([selected.id]) }
            }
            .keyboardShortcut(.delete, modifiers: .command)
            .disabled(selected == nil)

            Divider()

            Button("Choose Download Folder…") { model.chooseDestination() }
                .keyboardShortcut("o", modifiers: [.command, .shift])
            Button("Check Tools Again") { model.refreshTools() }
                .keyboardShortcut("r", modifiers: .command)
        }

        CommandGroup(after: .sidebar) {
            Button(model.showInspector ? "Hide Inspector" : "Show Inspector") { model.showInspector.toggle() }
                .keyboardShortcut("i", modifiers: [.command, .option])
            Divider()
            ForEach(Array(model.visibleFilters.prefix(5).enumerated()), id: \.element) { index, filter in
                Button("Show \(filter.title)") { model.filter = filter }
                    .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: .command)
            }
        }
    }

    private func primaryTitle(_ item: DownloadItem?) -> String {
        switch item?.status {
        case .kept: "Open"
        case .pulling, .polishing: "Pause"
        case .paused: "Resume"
        case .failed: "Retry"
        case .seeding: "Stop Seeding"
        case .queued: "Download Now"
        case nil: "Retry or Resume"
        }
    }
}
