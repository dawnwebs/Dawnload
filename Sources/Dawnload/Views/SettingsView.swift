import AppKit
import DawnloadCore
import SwiftUI

enum SettingsTab: String {
    case general, access, tools, torrents
}

/// Opens the Settings window on a tab.
enum SettingsOpener {
    static func open(tab: SettingsTab) {
        UserDefaults.standard.set(tab.rawValue, forKey: "settingsTab")
        NSApp.activate(ignoringOtherApps: true)
        if #available(macOS 14, *) {
            NSApp.sendAction(Selector(("showSettingsWindow:")), to: nil, from: nil)
        } else {
            NSApp.sendAction(Selector(("showPreferencesWindow:")), to: nil, from: nil)
        }
    }
}

struct SettingsView: View {
    @EnvironmentObject private var model: AppModel
    @AppStorage("settingsTab") private var tab = SettingsTab.general.rawValue

    var body: some View {
        TabView(selection: $tab) {
            GeneralSettings()
                .tabItem { Label("General", systemImage: "gearshape") }
                .tag(SettingsTab.general.rawValue)
            AccessSettings()
                .tabItem { Label("Access", systemImage: "person.badge.key") }
                .tag(SettingsTab.access.rawValue)
            ToolsSettings()
                .tabItem { Label("Tools", systemImage: "wrench.and.screwdriver") }
                .tag(SettingsTab.tools.rawValue)
            if model.torrentsEnabled {
                TorrentSettings()
                    .tabItem { Label("Torrents", systemImage: "point.3.connected.trianglepath.dotted") }
                    .tag(SettingsTab.torrents.rawValue)
            }
        }
        .frame(width: 540)
        .tint(DLColor.link)
        .preferredColorScheme(model.appearance.colorScheme)
    }
}

private struct GeneralSettings: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Form {
            Section {
                LabeledContent("Download folder") {
                    HStack {
                        Text(model.draft.destination.path)
                            .font(DLFont.mono(11))
                            .lineLimit(1)
                            .truncationMode(.middle)
                            .textSelection(.enabled)
                        Button("Choose…") { model.chooseDestination() }
                    }
                }
            }

            Section {
                Picker("Format", selection: $model.draft.quality) {
                    ForEach(QualityPreference.allCases) { Text($0.title).tag($0) }
                }
                Picker("Engine", selection: $model.draft.engine) {
                    Text("Auto").tag(EnginePreference.auto)
                    Text("yt-dlp").tag(EnginePreference.ytDlp)
                    Text("gallery-dl").tag(EnginePreference.galleryDl)
                }
                Toggle("Polish with ffmpeg", isOn: $model.draft.polish)
                Toggle("Download whole playlists", isOn: $model.draft.playlist)
            } header: {
                Text("New downloads")
            } footer: {
                Text("The chips under the paste field change these too. Each download keeps the options it was added with.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                Picker("Appearance", selection: $model.appearance) {
                    ForEach(AppearancePreference.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
            }
        }
        .formStyle(.grouped)
        .frame(height: 430)
    }
}

private struct AccessSettings: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Form {
            Section {
                Toggle("Impersonate a browser", isOn: $model.draft.impersonate)
                Picker("Browser", selection: $model.draft.impersonationTarget) {
                    ForEach(BrowserImpersonationTarget.allCases) { Text($0.title).tag($0) }
                }
                .disabled(!model.draft.impersonate)
            } header: {
                Text("Browser impersonation")
            } footer: {
                Text("yt-dlp presents itself as this browser, for sites that refuse plain requests.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section {
                if let cookies = model.cookiesFile {
                    LabeledContent("cookies.txt") {
                        HStack {
                            Text(cookies.lastPathComponent)
                                .font(DLFont.mono(11))
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Button("Replace…") { model.chooseCookiesFile() }
                            Button("Remove") { model.clearCookiesFile() }
                        }
                    }
                } else {
                    LabeledContent("cookies.txt") {
                        Button("Choose…") { model.chooseCookiesFile() }
                    }
                }
            } header: {
                Text("Cookies")
            } footer: {
                Text("Export cookies in Netscape format from a browser session you own. Dawnload passes the file to yt-dlp on this Mac and nowhere else.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section("When a site says HTTP 403") {
                VStack(alignment: .leading, spacing: 6) {
                    Text("1. Turn on impersonation with Chrome 146.")
                    Text("2. Try Chrome 145, Chrome 142, then Safari 18.4.")
                    Text("3. Still refused: add your cookies.txt above.")
                    Text("4. Use the clean page link, without search words after it.")
                }
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .frame(height: 500)
    }
}

private struct ToolsSettings: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Form {
            Section {
                ForEach(model.tools) { status in
                    VStack(alignment: .leading, spacing: 4) {
                        ToolLine(status: status)
                        Text(status.tool.purpose)
                            .font(.system(size: 11))
                            .foregroundStyle(.secondary)
                            .padding(.leading, 23)
                    }
                    .padding(.vertical, 2)
                }
            } header: {
                Text("Found on this Mac")
            } footer: {
                Text("Dawnload looks in /opt/homebrew/bin, /usr/local/bin and your PATH.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }

            Section("Install or update with Homebrew") {
                CommandBox(command: "brew install yt-dlp gallery-dl ffmpeg")
                CommandBox(command: "brew upgrade yt-dlp gallery-dl ffmpeg")
            }

            Section {
                HStack {
                    Spacer()
                    Button("Check Again") { model.refreshTools() }
                        .keyboardShortcut("r", modifiers: .command)
                }
            }
        }
        .formStyle(.grouped)
        .frame(height: 540)
    }
}

/// Shown only with torrents turned on (-DawnloadTorrents YES) until an engine ships.
private struct TorrentSettings: View {
    @EnvironmentObject private var model: AppModel

    var body: some View {
        Form {
            Section {
                Text("Torrent downloads aren't built yet. These are the settings they will use.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Section("Seeding") {
                Picker("Seed until ratio", selection: Binding(
                    get: { model.draft.seedRatioLimit ?? 0 },
                    set: { model.draft.seedRatioLimit = $0 == 0 ? nil : $0 }
                )) {
                    Text("0.5").tag(0.5)
                    Text("1.0").tag(1.0)
                    Text("2.0").tag(2.0)
                    Text("Until I stop it").tag(0.0)
                }
                Toggle("Ask which files before downloading", isOn: $model.draft.askForFiles)
            }
            Section {
                Text("Only download and seed torrents you have the right to share. Seeding uploads pieces of the files to other people.")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .formStyle(.grouped)
        .frame(height: 360)
        .disabled(true)
    }
}

extension AppearancePreference {
    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .dark: .dark
        case .light: .light
        }
    }
}
