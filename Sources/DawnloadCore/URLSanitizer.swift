import Foundation

public func normalizedDownloadURL(from text: String) -> URL? {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    guard var components = URLComponents(string: trimmed),
          let scheme = components.scheme?.lowercased(),
          scheme == "http" || scheme == "https",
          let host = components.host?.lowercased(),
          !host.isEmpty else {
        return nil
    }

    if host == "spankbang.com" || host.hasSuffix(".spankbang.com") {
        components.query = nil
        components.fragment = nil
    }

    return components.url
}
