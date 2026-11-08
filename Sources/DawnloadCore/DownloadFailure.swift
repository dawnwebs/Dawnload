import Foundation

/// An error in the brand's voice: what happened, then what to do.
public struct DownloadFailure: Codable, Equatable, Error {
    public enum Kind: String, Codable {
        case forbidden
        case signInNeeded
        case notFound
        case unsupportedSite
        case toolMissing
        case ffmpegNeeded
        case cookiesUnreadable
        case notALink
        case engineUnavailable
        case folderUnavailable
        case other
    }

    public var kind: Kind
    /// What happened, one sentence. Shown on the row.
    public var message: String
    /// What to do, one sentence. Shown in the inspector.
    public var hint: String?
    /// The tool's own last error line, if there was one.
    public var detail: String?

    public init(kind: Kind, message: String, hint: String? = nil, detail: String? = nil) {
        self.kind = kind
        self.message = message
        self.hint = hint
        self.detail = detail
    }

    public var sentence: String {
        [message, hint].compactMap { $0 }.joined(separator: " ")
    }

    public static func toolMissing(_ tool: String) -> DownloadFailure {
        DownloadFailure(kind: .toolMissing, message: "\(tool) isn't installed.",
                        hint: "Install it with Homebrew, then check again.")
    }

    public static let ffmpegNeeded = DownloadFailure(
        kind: .ffmpegNeeded, message: "Audio only needs ffmpeg.", hint: "Install ffmpeg, or choose Best.")

    public static let cookiesUnreadable = DownloadFailure(
        kind: .cookiesUnreadable, message: "The cookies file can't be read any more.", hint: "Choose it again in Settings.")

    public static let notALink = DownloadFailure(
        kind: .notALink, message: "That isn't a full link.", hint: "Paste one that starts with https://.")

    public static let torrentsUnavailable = DownloadFailure(
        kind: .engineUnavailable, message: "Torrent downloads aren't built yet.",
        hint: "Dawnload can read this link, but it has nothing to download torrents with yet.")

    public static func folderUnavailable(_ path: String) -> DownloadFailure {
        DownloadFailure(kind: .folderUnavailable, message: "Dawnload can't write to \(path).",
                        hint: "Choose another folder in Settings.")
    }

    /// Reads a tool's output after it exited with an error.
    public static func classify(tool: String, exitCode: Int32, output: String) -> DownloadFailure {
        let detail = lastErrorLine(in: output)
        let haystack = output.lowercased()

        func has(_ needles: String...) -> Bool {
            needles.contains { haystack.contains($0.lowercased()) }
        }

        if has("http error 403", "403: forbidden", "403 forbidden") {
            return DownloadFailure(kind: .forbidden, message: "This site refused the request (HTTP 403).",
                                   hint: "Turn on browser impersonation, or add your cookies.txt in Settings.", detail: detail)
        }
        if has("sign in to confirm", "login required", "requires login", "log in to", "use --cookies",
               "private video", "this video is private", "members-only", "authentication required", "http error 401") {
            return DownloadFailure(kind: .signInNeeded, message: "This needs you to be signed in.",
                                   hint: "Add your cookies.txt in Settings.", detail: detail)
        }
        if has("http error 404", "404: not found", "404 not found") {
            return DownloadFailure(kind: .notFound, message: "The page wasn't found (HTTP 404).",
                                   hint: "Check the link.", detail: detail)
        }
        if has("unsupported url", "no suitable extractor") {
            let hint = tool == "gallery-dl"
                ? "Try yt-dlp for video and audio."
                : "Try gallery-dl for galleries and image sets."
            return DownloadFailure(kind: .unsupportedSite, message: "\(tool) doesn't support this site.", hint: hint, detail: detail)
        }
        if has("ffmpeg not found", "ffprobe and ffmpeg not found", "ffmpeg is not installed") {
            return DownloadFailure(kind: .ffmpegNeeded, message: "This download needs ffmpeg.",
                                   hint: "Install ffmpeg, or turn off Polish.", detail: detail)
        }

        return DownloadFailure(kind: .other, message: "\(tool) stopped with an error (exit \(exitCode)).",
                               hint: "The log has the details.", detail: detail)
    }

    static func lastErrorLine(in output: String) -> String? {
        output.components(separatedBy: .newlines)
            .last { $0.hasPrefix("ERROR:") || $0.contains("[error]") }
            .map { $0.trimmingCharacters(in: .whitespaces) }
    }
}
