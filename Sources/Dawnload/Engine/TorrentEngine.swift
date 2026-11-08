import DawnloadCore
import Foundation

/// The torrent seam. It accepts magnet links and .torrent files so the intake, the model and the
/// screens can treat them as items, and reports itself unavailable until an engine is chosen.
///
/// To add one (aria2c over JSON-RPC, transmission-daemon, or libtorrent; see the options in
/// docs/DESIGN.md › Torrents), give it a `ToolID` if it's a tool on the Mac, return `.ready` from
/// `availability`, and return a job from `makeJob` that:
/// - emits `.files` as soon as the metadata arrives (and honours `request.selectedFiles`),
/// - emits `.transfer` and `.swarm` while pulling, `.phase(.pulling, detail: "Checking")` while
///   verifying pieces,
/// - returns `JobOutcome(continuesSeeding: true)` when the files are on disk, then keeps emitting
///   `.swarm` until the ratio in `options.seedRatioLimit` is reached or `stop()` is called.
struct TorrentEngine: DownloadEngine {
    let id = EngineID.torrent
    let displayName = "Torrent"
    let lane = EngineLane.torrent
    let capabilities: EngineCapabilities = [.resume, .seeding, .fileSelection, .swarm]

    func accepts(_ source: DownloadSource) -> Bool {
        source.isTorrent
    }

    func availability(tools: ToolSnapshot, options: DownloadOptions) -> EngineAvailability {
        .notBuilt
    }

    func makeJob(for request: JobRequest) throws -> DownloadJob {
        throw DownloadFailure.torrentsUnavailable
    }
}
