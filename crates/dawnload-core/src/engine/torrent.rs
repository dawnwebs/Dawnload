//! The torrent seam. It accepts magnet links, `.torrent` files and `.torrent` links so intake, the
//! queue and the screens treat them as items, and reports "not built yet" until librqbit lands
//! (Phase 4, see docs/adr/0003-seeding-inside-the-job.md for the contract a real engine follows).
//!
//! A real engine's `run`:
//! - emits `Files` as soon as metadata arrives and honours unticked files in `request.files`,
//! - emits `Transfer` and `Swarm` while pulling, `Phase(Pulling, "Checking")` while verifying,
//! - emits `Phase(Seeding)` once the files are on disk and keeps emitting `Swarm` while it
//!   uploads; the queue then frees its download slot,
//! - returns `Ok` at `options.seed_ratio_limit`, or `Err(Stopped)` on Stop Seeding / Pause.

use super::*;

pub struct TorrentEngine;

impl DownloadEngine for TorrentEngine {
    fn id(&self) -> EngineId {
        EngineId::Torrent
    }

    fn lane(&self) -> Lane {
        Lane::Torrent
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::RESUME | Capabilities::SEEDING | Capabilities::FILE_SELECTION | Capabilities::SWARM
    }

    fn accepts(&self, source: &DownloadSource) -> bool {
        match source {
            DownloadSource::Magnet(_) | DownloadSource::TorrentFile { .. } => true,
            DownloadSource::Web { url } => {
                url::Url::parse(url).ok().and_then(|u| crate::source::url_file_extension(&u)).as_deref() == Some("torrent")
            }
        }
    }

    fn availability(&self, _tools: &ToolSnapshot, _options: &Options) -> Availability {
        Availability::NotBuilt
    }

    fn run(&self, _request: JobRequest, _context: JobContext) -> JobFuture {
        Box::pin(async { Err(JobError::Failed(DownloadFailure::torrents_unavailable())) })
    }
}
