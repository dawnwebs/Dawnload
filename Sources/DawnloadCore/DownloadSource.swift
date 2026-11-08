import Foundation

/// Where a download comes from. The paste field, drops and Open all produce one of these.
public enum DownloadSource: Codable, Equatable, Hashable {
    /// An http(s) page that yt-dlp or gallery-dl reads.
    case web(URL)
    /// A `magnet:` link. Needs a torrent engine.
    case magnet(MagnetLink)
    /// A `.torrent` file on disk. Needs a torrent engine.
    case torrentFile(URL)

    /// Reads pasted or dropped text. Returns nil when it isn't something Dawnload can download.
    public static func parse(_ text: String) -> DownloadSource? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        if trimmed.lowercased().hasPrefix("magnet:") {
            return MagnetLink(trimmed).map(DownloadSource.magnet)
        }

        if let fileURL = torrentFileURL(from: trimmed) {
            return .torrentFile(fileURL)
        }

        return normalizedDownloadURL(from: trimmed).map(DownloadSource.web)
    }

    public var kind: Kind {
        switch self {
        case .web: .web
        case .magnet: .magnet
        case .torrentFile: .torrentFile
        }
    }

    public var isTorrent: Bool { kind != .web }

    /// The string to show and to copy: the URL, the magnet link, or the file path.
    public var absoluteString: String {
        switch self {
        case .web(let url): url.absoluteString
        case .magnet(let magnet): magnet.uri
        case .torrentFile(let url): url.path
        }
    }

    /// Host for web links, the magnet's name, or the file name.
    public var shortName: String {
        switch self {
        case .web(let url):
            return url.host.map(Self.trimmedHost) ?? url.absoluteString
        case .magnet(let magnet):
            return magnet.displayName ?? "Magnet link"
        case .torrentFile(let url):
            return url.lastPathComponent
        }
    }

    /// host + path for web links, used as a title before the tool reports a real one.
    public var provisionalTitle: String {
        switch self {
        case .web(let url):
            guard let host = url.host else { return url.absoluteString }
            let path = url.path == "/" ? "" : url.path
            return Self.trimmedHost(host) + path
        case .magnet(let magnet):
            return magnet.displayName ?? "Magnet \(magnet.infoHash.prefix(12))"
        case .torrentFile(let url):
            return url.deletingPathExtension().lastPathComponent
        }
    }

    public enum Kind: String, Codable {
        case web
        case magnet
        case torrentFile
    }

    static func trimmedHost(_ host: String) -> String {
        let lower = host.lowercased()
        return lower.hasPrefix("www.") ? String(lower.dropFirst(4)) : lower
    }

    private static func torrentFileURL(from text: String) -> URL? {
        let url: URL
        if text.lowercased().hasPrefix("file://") {
            guard let parsed = URL(string: text), parsed.isFileURL else { return nil }
            url = parsed
        } else if text.hasPrefix("/") || text.hasPrefix("~") {
            url = URL(fileURLWithPath: (text as NSString).expandingTildeInPath)
        } else {
            return nil
        }

        return url.pathExtension.lowercased() == "torrent" ? url.standardizedFileURL : nil
    }
}

/// A parsed `magnet:?xt=urn:btih:…` link.
public struct MagnetLink: Codable, Equatable, Hashable {
    public let uri: String
    /// Lowercase hex (v1, 40 chars), base32 as given (32 chars), or a v2 multihash.
    public let infoHash: String
    public let displayName: String?
    public let trackers: [String]
    public let exactLength: Int64?

    public init?(_ text: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.lowercased().hasPrefix("magnet:?"),
              let components = URLComponents(string: trimmed) else {
            return nil
        }

        let items = components.queryItems ?? []
        var hash: String?
        for item in items where item.name.lowercased() == "xt" {
            guard let value = item.value else { continue }
            let lower = value.lowercased()
            if lower.hasPrefix("urn:btih:") {
                let raw = String(value.dropFirst("urn:btih:".count))
                hash = raw.count == 40 ? raw.lowercased() : raw.uppercased()
                break
            }
            if lower.hasPrefix("urn:btmh:") {
                hash = String(value.dropFirst("urn:btmh:".count)).lowercased()
                break
            }
        }

        guard let infoHash = hash, Self.isValidHash(infoHash) else {
            return nil
        }

        self.uri = trimmed
        self.infoHash = infoHash
        self.displayName = items.first { $0.name.lowercased() == "dn" }?.value
            .flatMap { $0.replacingOccurrences(of: "+", with: " ") }
            .flatMap { $0.trimmingCharacters(in: .whitespaces).isEmpty ? nil : $0 }
        self.trackers = items.filter { $0.name.lowercased() == "tr" }.compactMap(\.value)
        self.exactLength = items.first { $0.name.lowercased() == "xl" }?.value.flatMap { Int64($0) }
    }

    private static func isValidHash(_ hash: String) -> Bool {
        let hex = CharacterSet(charactersIn: "0123456789abcdef")
        let base32 = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567")
        switch hash.count {
        case 40:
            return hash.unicodeScalars.allSatisfy(hex.contains)
        case 32:
            return hash.unicodeScalars.allSatisfy(base32.contains)
        default:
            // v2 multihash: 1220 + 64 hex chars
            return hash.count == 68 && hash.hasPrefix("1220") && hash.unicodeScalars.allSatisfy(hex.contains)
        }
    }
}
