import Foundation

/// The engines Dawnload knows. `torrent` has no implementation yet.
public enum EngineID: String, Codable, CaseIterable, Identifiable {
    case ytDlp = "yt-dlp"
    case galleryDl = "gallery-dl"
    case torrent

    public var id: String { rawValue }
}

/// What the Engine chip is set to. `auto` resolves per link.
public enum EnginePreference: String, Codable, CaseIterable, Identifiable {
    case auto
    case ytDlp
    case galleryDl

    public var id: String { rawValue }

    public func resolve(for source: DownloadSource) -> EngineID {
        switch source {
        case .magnet, .torrentFile:
            return .torrent
        case .web(let url):
            switch self {
            case .ytDlp: return .ytDlp
            case .galleryDl: return .galleryDl
            case .auto: return EngineSuggestion.webEngine(for: url)
            }
        }
    }
}

public enum EngineSuggestion {
    /// Sites where gallery-dl does the better job. Everything else goes to yt-dlp.
    static let galleryHosts: Set<String> = [
        "artstation.com", "behance.net", "bunkr.si", "danbooru.donmai.us", "deviantart.com",
        "e621.net", "flickr.com", "gelbooru.com", "imgur.com", "kemono.su", "pinterest.com",
        "pixiv.net", "safebooru.org", "wallhaven.cc", "500px.com", "tumblr.com"
    ]

    public static func webEngine(for url: URL) -> EngineID {
        guard let host = url.host.map(DownloadSource.trimmedHost) else { return .ytDlp }
        let path = url.path.lowercased()

        if host == "instagram.com" || host.hasSuffix(".instagram.com") {
            let videoPaths = ["/reel/", "/reels/", "/tv/"]
            return videoPaths.contains { path.hasPrefix($0) } ? .ytDlp : .galleryDl
        }

        let matches = galleryHosts.contains { host == $0 || host.hasSuffix("." + $0) }
        return matches ? .galleryDl : .ytDlp
    }
}
