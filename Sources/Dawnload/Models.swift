import Foundation

enum DownloadMode: String, CaseIterable, Identifiable {
    case ytDlp
    case galleryDl

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .ytDlp:
            "yt-dlp"
        case .galleryDl:
            "gallery-dl"
        }
    }

    var executableName: String {
        switch self {
        case .ytDlp:
            "yt-dlp"
        case .galleryDl:
            "gallery-dl"
        }
    }

    var helpText: String {
        switch self {
        case .ytDlp:
            "Best for video sites, playlists, and audio/video merging."
        case .galleryDl:
            "Best for gallery and social media media sets."
        }
    }
}

enum QualityPreference: String, CaseIterable, Identifiable {
    case best
    case compatible
    case audio

    var id: String { rawValue }

    var title: String {
        switch self {
        case .best:
            "Best"
        case .compatible:
            "Compatible"
        case .audio:
            "Audio"
        }
    }

    var subtitle: String {
        switch self {
        case .best:
            "Highest quality"
        case .compatible:
            "MP4/M4A preferred"
        case .audio:
            "M4A audio only"
        }
    }
}

enum BrowserImpersonationTarget: String, CaseIterable, Identifiable {
    case chrome146 = "chrome-146"
    case chrome145 = "chrome-145"
    case chrome142 = "chrome-142"
    case safari184 = "safari-18.4"

    var id: String { rawValue }

    var title: String {
        switch self {
        case .chrome146:
            "Chrome 146"
        case .chrome145:
            "Chrome 145"
        case .chrome142:
            "Chrome 142"
        case .safari184:
            "Safari 18.4"
        }
    }

    var compactTitle: String {
        switch self {
        case .chrome146:
            "chrome 146"
        case .chrome145:
            "chrome 145"
        case .chrome142:
            "chrome 142"
        case .safari184:
            "safari 18.4"
        }
    }
}

enum RunState: Equatable {
    case idle
    case queued(String)
    case running(String)
    case completed(String)
    case failed(String)
    case cancelled

    var isRunning: Bool {
        if case .running = self {
            true
        } else {
            false
        }
    }

    var title: String {
        switch self {
        case .idle:
            "Ready"
        case .queued:
            "Queued"
        case .running(let phase):
            phase
        case .completed:
            "Download complete"
        case .failed:
            "Could not finish"
        case .cancelled:
            "Cancelled"
        }
    }

    var symbolName: String {
        switch self {
        case .idle:
            "sparkles"
        case .queued:
            "text.badge.plus"
        case .running:
            "arrow.down.circle.fill"
        case .completed:
            "checkmark.circle.fill"
        case .failed:
            "exclamationmark.circle.fill"
        case .cancelled:
            "pause.circle.fill"
        }
    }
}

struct ToolStatus: Identifiable, Equatable {
    let name: String
    let path: String?

    var id: String { name }
    var isInstalled: Bool { path != nil }
}

struct DownloadQueueItem: Identifiable, Equatable {
    let id: UUID
    let url: String
    let mode: DownloadMode
    let quality: QualityPreference
    let destination: URL
    let useFFmpeg: Bool
    let useBrowserImpersonation: Bool
    let impersonationTarget: BrowserImpersonationTarget
    let cookiesFile: URL?
    let useYesPlaylist: Bool
    let createdAt: Date

    init(
        id: UUID = UUID(),
        url: String,
        mode: DownloadMode,
        quality: QualityPreference,
        destination: URL,
        useFFmpeg: Bool,
        useBrowserImpersonation: Bool,
        impersonationTarget: BrowserImpersonationTarget,
        cookiesFile: URL?,
        useYesPlaylist: Bool,
        createdAt: Date = Date()
    ) {
        self.id = id
        self.url = url
        self.mode = mode
        self.quality = quality
        self.destination = destination
        self.useFFmpeg = useFFmpeg
        self.useBrowserImpersonation = useBrowserImpersonation
        self.impersonationTarget = impersonationTarget
        self.cookiesFile = cookiesFile
        self.useYesPlaylist = useYesPlaylist
        self.createdAt = createdAt
    }

    var displayURL: String {
        guard let parsed = URL(string: url), let host = parsed.host else {
            return url
        }

        let path = parsed.path == "/" ? "" : parsed.path
        return host + path
    }

    var settingsSummary: String {
        var parts = [mode.displayName]

        if mode == .ytDlp {
            parts.append(quality.title)
        }

        if useFFmpeg {
            parts.append("ffmpeg")
        }

        if useBrowserImpersonation && mode == .ytDlp {
            parts.append(impersonationTarget.title)
        }

        if cookiesFile != nil && mode == .ytDlp {
            parts.append("Cookies")
        }

        if useYesPlaylist && mode == .ytDlp {
            parts.append("Playlist")
        }

        return parts.joined(separator: " · ")
    }
}

struct DownloadRequest {
    let url: String
    let mode: DownloadMode
    let quality: QualityPreference
    let destination: URL
    let ffmpegPath: String?
    let useBrowserImpersonation: Bool
    let impersonationTarget: BrowserImpersonationTarget
    let cookiesFile: URL?
    let useYesPlaylist: Bool
    let environment: [String: String]
}

struct DownloadSummary {
    let finalizedFiles: Int
}

enum DownloadEvent {
    case command(String)
    case line(String)
    case phase(String)
    case progress(Double?)
}
