import Foundation

public struct SmartFilenameMetadata: Decodable, Equatable {
    public let id: String?
    public let title: String?
    public let fullTitle: String?
    public let displayID: String?
    public let uploader: String?
    public let uploadDate: String?
    public let webpageURL: String?
    public let filepath: String?

    public init(
        id: String? = nil,
        title: String? = nil,
        fullTitle: String? = nil,
        displayID: String? = nil,
        uploader: String? = nil,
        uploadDate: String? = nil,
        webpageURL: String? = nil,
        filepath: String? = nil
    ) {
        self.id = id
        self.title = title
        self.fullTitle = fullTitle
        self.displayID = displayID
        self.uploader = uploader
        self.uploadDate = uploadDate
        self.webpageURL = webpageURL
        self.filepath = filepath
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case title
        case fullTitle = "fulltitle"
        case displayID = "display_id"
        case uploader
        case uploadDate = "upload_date"
        case webpageURL = "webpage_url"
        case filepath
    }
}

public enum SmartFilenameResolver {
    public static func bestTitle(
        metadata: SmartFilenameMetadata,
        sourceURL: String
    ) -> String {
        let rejectedKeys = Set([
            normalizedKey(metadata.id),
            normalizedKey(metadata.displayID),
            normalizedKey(metadata.uploader)
        ].filter { !$0.isEmpty })

        for candidate in [metadata.fullTitle, metadata.title] {
            if let title = meaningfulTitle(candidate, rejectedKeys: rejectedKeys) {
                return title
            }
        }

        for urlString in [metadata.webpageURL, sourceURL] {
            if let slug = titleSlug(from: urlString),
               let title = meaningfulTitle(slug, rejectedKeys: rejectedKeys) {
                return title
            }
        }

        if let uploader = meaningfulTitle(metadata.uploader, rejectedKeys: []),
           let date = displayDate(metadata.uploadDate) {
            return "\(uploader) — \(date)"
        }

        if let identifier = cleaned(metadata.id ?? metadata.displayID),
           !identifier.isEmpty {
            return "Video \(identifier)"
        }

        return "Downloaded Video"
    }

    private static func meaningfulTitle(
        _ value: String?,
        rejectedKeys: Set<String>
    ) -> String? {
        guard let title = cleaned(value),
              title.count >= 2 else {
            return nil
        }

        let key = normalizedKey(title)
        let genericKeys: Set<String> = [
            "download",
            "downloadedvideo",
            "media",
            "untitled",
            "video",
            "watch"
        ]

        guard !key.isEmpty,
              !genericKeys.contains(key),
              !rejectedKeys.contains(key),
              !looksLikeURL(title) else {
            return nil
        }

        return title
    }

    private static func titleSlug(from urlString: String?) -> String? {
        guard let urlString,
              let components = URLComponents(string: urlString) else {
            return nil
        }

        let segments = components.percentEncodedPath
            .split(separator: "/", omittingEmptySubsequences: true)
            .map(String.init)

        let titleMarkers: Set<String> = [
            "clip",
            "clips",
            "play",
            "post",
            "posts",
            "reel",
            "reels",
            "short",
            "shorts",
            "status",
            "statuses",
            "v",
            "video",
            "videos",
            "watch"
        ]

        for index in segments.indices {
            guard index < segments.index(before: segments.endIndex),
                  titleMarkers.contains(segments[index].lowercased()) else {
                continue
            }

            let titleIndex = segments.index(after: index)
            return readableSlug(segments[titleIndex])
        }

        return nil
    }

    private static func readableSlug(_ slug: String) -> String? {
        let decoded = slug.removingPercentEncoding ?? slug
        let separated = decoded.replacingOccurrences(
            of: #"[-_+]+"#,
            with: " ",
            options: .regularExpression
        )
        guard let cleanedSlug = cleaned(separated) else {
            return nil
        }

        return cleanedSlug
            .split(separator: " ")
            .map(formatSlugWord)
            .joined(separator: " ")
    }

    private static func formatSlugWord(_ word: Substring) -> String {
        let value = String(word)
        let containsNumber = value.contains(where: \.isNumber)
        let containsOnlyLetters = value.allSatisfy(\.isLetter)

        if containsNumber || (containsOnlyLetters && value.count <= 3) {
            return value.uppercased()
        }

        return value.prefix(1).uppercased() + value.dropFirst().lowercased()
    }

    private static func cleaned(_ value: String?) -> String? {
        guard let value else {
            return nil
        }

        let cleaned = value
            .precomposedStringWithCanonicalMapping
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression)
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .trimmingCharacters(in: CharacterSet(charactersIn: ".-_"))

        return cleaned.isEmpty ? nil : cleaned
    }

    private static func normalizedKey(_ value: String?) -> String {
        guard let value else {
            return ""
        }

        return String(value.lowercased().unicodeScalars.filter {
            CharacterSet.alphanumerics.contains($0)
        })
    }

    private static func looksLikeURL(_ value: String) -> Bool {
        let lowercased = value.lowercased()
        return lowercased.hasPrefix("http://")
            || lowercased.hasPrefix("https://")
            || lowercased.hasPrefix("www.")
    }

    private static func displayDate(_ uploadDate: String?) -> String? {
        guard let uploadDate,
              uploadDate.count == 8,
              uploadDate.allSatisfy(\.isNumber) else {
            return nil
        }

        let yearEnd = uploadDate.index(uploadDate.startIndex, offsetBy: 4)
        let monthEnd = uploadDate.index(yearEnd, offsetBy: 2)
        return "\(uploadDate[..<yearEnd])-\(uploadDate[yearEnd..<monthEnd])-\(uploadDate[monthEnd...])"
    }
}
