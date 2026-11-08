import DawnloadCore
import Foundation

// The seam every download goes through. yt-dlp and gallery-dl are engines today; a torrent engine
// plugs in here without the model or the screens changing. See docs/DESIGN.md › Torrents.

/// What an engine can do. The UI shows a control only when the engine has the capability.
struct EngineCapabilities: OptionSet {
    let rawValue: Int

    /// Picks a format (Best, Compatible, Audio only).
    static let formats = EngineCapabilities(rawValue: 1 << 0)
    /// Follows a playlist link to every entry.
    static let playlists = EngineCapabilities(rawValue: 1 << 1)
    /// Can present itself as a browser and send cookies.
    static let siteAccess = EngineCapabilities(rawValue: 1 << 2)
    /// Runs ffmpeg after the download.
    static let polish = EngineCapabilities(rawValue: 1 << 3)
    /// Picks up where it stopped after Pause.
    static let resume = EngineCapabilities(rawValue: 1 << 4)
    /// Keeps uploading after Kept.
    static let seeding = EngineCapabilities(rawValue: 1 << 5)
    /// Lets the person skip some of an item's files.
    static let fileSelection = EngineCapabilities(rawValue: 1 << 6)
    /// Reports peers, seeds, upload and ratio.
    static let swarm = EngineCapabilities(rawValue: 1 << 7)
}

/// Items in one lane run one after another (up to `maxConcurrent`); lanes run side by side.
enum EngineLane: String {
    case web
    case torrent

    var maxConcurrent: Int {
        switch self {
        case .web: 1
        case .torrent: 3
        }
    }
}

enum EngineAvailability: Equatable {
    case ready
    /// A tool it runs isn't on this Mac.
    case missingTool(ToolID)
    /// Dawnload has no implementation yet.
    case notBuilt

    var failure: DownloadFailure? {
        switch self {
        case .ready: nil
        case .missingTool(let tool): .toolMissing(tool.rawValue)
        case .notBuilt: .torrentsUnavailable
        }
    }
}

/// Everything a job needs to run one item.
struct JobRequest {
    let itemID: UUID
    let source: DownloadSource
    let options: DownloadOptions
    let tools: ToolSnapshot
    /// Torrents: the paths to download; nil means all.
    let selectedFiles: [String]?
}

/// What a running job reports. Events arrive on any thread; the model applies them on the main actor.
enum JobEvent {
    case command(String)
    case log(String)
    case phase(ItemStatus, detail: String?)
    case transfer(TransferSample)
    case title(String)
    case files([ItemFile])
    case swarm(SwarmStats)
}

struct JobOutcome {
    var files: [URL]
    var polishedCount = 0
    /// Torrents: the files are kept and the job keeps running to seed.
    var continuesSeeding = false
}

/// Thrown by a job that was stopped by Pause or Remove.
struct JobStopped: Error {}

protocol DownloadJob: AnyObject {
    /// Runs until the item is kept, fails (throws `DownloadFailure`) or is stopped (throws `JobStopped`).
    func run(emit: @escaping (JobEvent) -> Void) async throws -> JobOutcome
    /// Stops the job. Partial files stay, so a later run can continue from them.
    func stop()
}

protocol DownloadEngine {
    var id: EngineID { get }
    var displayName: String { get }
    var lane: EngineLane { get }
    var capabilities: EngineCapabilities { get }
    func accepts(_ source: DownloadSource) -> Bool
    func availability(tools: ToolSnapshot, options: DownloadOptions) -> EngineAvailability
    func makeJob(for request: JobRequest) throws -> DownloadJob
}

enum EngineRegistry {
    static let all: [DownloadEngine] = [YTDlpEngine(), GalleryDlEngine(), TorrentEngine()]

    static func engine(_ id: EngineID) -> DownloadEngine {
        all.first { $0.id == id }!
    }
}
