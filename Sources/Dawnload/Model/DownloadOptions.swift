import DawnloadCore
import Foundation

enum QualityPreference: String, Codable, CaseIterable, Identifiable {
    case best
    case compatible
    case audio

    var id: String { rawValue }

    var title: String {
        switch self {
        case .best: "Best"
        case .compatible: "Compatible"
        case .audio: "Audio only"
        }
    }

    var subtitle: String {
        switch self {
        case .best: "Highest quality the site offers"
        case .compatible: "MP4 and M4A, plays everywhere"
        case .audio: "M4A audio, needs ffmpeg"
        }
    }
}

enum BrowserImpersonationTarget: String, Codable, CaseIterable, Identifiable {
    case chrome146 = "chrome-146"
    case chrome145 = "chrome-145"
    case chrome142 = "chrome-142"
    case safari184 = "safari-18.4"

    var id: String { rawValue }

    var title: String {
        switch self {
        case .chrome146: "Chrome 146"
        case .chrome145: "Chrome 145"
        case .chrome142: "Chrome 142"
        case .safari184: "Safari 18.4"
        }
    }
}

/// How one item downloads. The intake chips edit a copy that sticks between launches; each item
/// keeps the copy it was added with.
struct DownloadOptions: Codable, Equatable {
    var engine: EnginePreference = .auto
    var quality: QualityPreference = .best
    var destination: URL
    /// ffmpeg merges, remuxes to MP4 and moves the index to the front.
    var polish = true
    var playlist = false
    var impersonate = false
    var impersonationTarget: BrowserImpersonationTarget = .chrome146
    /// Pass the cookies.txt from Settings to yt-dlp.
    var useCookies = true
    /// The cookies file at the time the item was added.
    var cookiesFile: URL?

    // Torrents (no engine yet; see docs/DESIGN.md › Torrents).
    /// Stop seeding at this upload ratio. nil seeds until stopped.
    var seedRatioLimit: Double? = 1.0
    /// Show the file list before the download starts.
    var askForFiles = false

    static var defaultDestination: URL {
        let downloads = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first
        return (downloads ?? URL(fileURLWithPath: NSHomeDirectory()))
            .appendingPathComponent("Dawnload", isDirectory: true)
    }

    init(destination: URL = DownloadOptions.defaultDestination) {
        self.destination = destination
    }

    /// The words on a Queued row and in the inspector: "Best · Polish · Chrome 146".
    func summary(for engine: EngineID) -> String {
        var parts: [String] = []
        switch engine {
        case .ytDlp:
            parts.append(quality.title)
            if polish { parts.append("Polish") }
            if playlist { parts.append("Playlist") }
            if impersonate { parts.append(impersonationTarget.title) }
            if useCookies, cookiesFile != nil { parts.append("cookies.txt") }
        case .galleryDl:
            parts.append("Original files")
            if polish { parts.append("Polish") }
        case .torrent:
            if let seedRatioLimit { parts.append("Seed to \(DLFormat.ratio(seedRatioLimit))") }
        }
        return parts.joined(separator: " · ")
    }
}
