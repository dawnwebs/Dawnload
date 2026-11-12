//! Fake downloads with believable numbers, for UI prototypes and load tests. Each job ticks a few
//! times a second; torrents pull, then seed to their ratio. Nothing touches the network or disk.

use std::time::Duration;

use super::*;
use crate::failure::FailureKind;
use crate::model::Swarm;

#[derive(Clone, Debug)]
pub struct DemoEngine {
    /// Event ticks per second per job.
    pub ticks_per_second: f64,
    /// 1.0 is real time; higher finishes sooner.
    pub time_scale: f64,
}

impl Default for DemoEngine {
    fn default() -> Self {
        DemoEngine { ticks_per_second: 5.0, time_scale: 1.0 }
    }
}

/// A small deterministic generator, so one item behaves the same every run.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// 0.0…1.0
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }
}

const TITLES: &[&str] = &[
    "Big Buck Bunny",
    "Sintel",
    "Tears of Steel",
    "Elephants Dream",
    "Spring",
    "Cosmos Laundromat",
    "Agent 327",
    "Caminandes",
    "Glass Half",
    "Charge",
    "Sprite Fright",
    "Hero",
    "Coffee Run",
    "Wing It",
    "Night of the Living Dead",
    "Ubuntu 26.04 Desktop",
    "Debian 13 netinst",
    "Fedora Workstation 45",
    "Open Movie Soundtrack",
    "Public Domain Archive",
    "NASA Earth at Night",
    "Apollo 11 Restored",
    "Lecture 12: Compilers",
    "Field Recording, Rain",
    "Podcast Episode 214",
];

pub fn demo_title(seed: u64) -> String {
    let mut rng = Rng::new(seed);
    let base = TITLES[(rng.next_u64() % TITLES.len() as u64) as usize];
    format!("{base} · part {}", 1 + rng.next_u64() % 40)
}

impl DownloadEngine for DemoEngine {
    fn id(&self) -> EngineId {
        EngineId::Demo
    }

    fn lane(&self) -> Lane {
        Lane::Demo
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::RESUME | Capabilities::SEEDING | Capabilities::SWARM | Capabilities::POLISH
    }

    fn accepts(&self, _source: &DownloadSource) -> bool {
        true
    }

    fn availability(&self, _tools: &ToolSnapshot, _options: &Options) -> Availability {
        Availability::Ready
    }

    fn run(&self, request: JobRequest, context: JobContext) -> JobFuture {
        Box::pin(run(self.clone(), request, context))
    }
}

async fn run(engine: DemoEngine, request: JobRequest, ctx: JobContext) -> JobResult {
    let mut rng = Rng::new(request.item_id.0);
    let torrent = request.source.is_torrent();
    // Web items 4 MB … 270 MB, torrents 32 MB … 2 GB: a few seconds to a few minutes each.
    let total = (rng.range(16.0, 22.0).exp2()) as u64 * if torrent { 512 } else { 64 };
    let rate = rng.range(0.6e6, 9.0e6) * engine.time_scale;
    let fails_at = (rng.unit() < 0.04).then(|| rng.range(0.2, 0.8));
    let polishes = !torrent && rng.unit() < 0.5;
    let tick = Duration::from_secs_f64(1.0 / engine.ticks_per_second.max(0.1));
    let path = request.options.destination.join(format!("demo-{}.{}", request.item_id, if torrent { "iso" } else { "mp4" }));
    let mut done = request.files.first().and_then(|f| f.bytes_done).unwrap_or(0).min(total);

    ctx.emit(JobEvent::Title(demo_title(request.item_id.0)));
    ctx.emit(JobEvent::Command(format!("demo --size {total} --rate {rate:.0}")));
    ctx.phase(Status::Pulling, None);
    let file = |done: u64| {
        vec![ItemFile { path: path.display().to_string(), size: Some(total), bytes_done: Some(done), selected: true }]
    };
    ctx.emit(JobEvent::Files(file(done)));

    let mut interval = tokio::time::interval(tick);
    let mut ticks = 0u64;
    while done < total {
        tokio::select! {
            _ = interval.tick() => {}
            _ = ctx.stop.stopped() => {
                ctx.emit(JobEvent::Files(file(done)));
                return Err(JobError::Stopped);
            }
        }
        ticks += 1;
        let jitter = rng.range(0.6, 1.4);
        let speed = rate * jitter;
        done = (done + (speed * tick.as_secs_f64()) as u64).min(total);
        let fraction = done as f64 / total as f64;
        if fails_at.is_some_and(|at| fraction >= at) {
            return Err(DownloadFailure::new(
                FailureKind::Forbidden,
                "This site refused the request (HTTP 403).",
                Some("Turn on browser impersonation, or add your cookies.txt in Settings."),
            )
            .with_detail("ERROR: demo: HTTP Error 403: Forbidden")
            .into());
        }
        ctx.emit(JobEvent::Transfer(Transfer {
            fraction: Some(fraction),
            bytes_done: Some(done),
            bytes_total: Some(total),
            bytes_per_second: Some(speed),
            seconds_left: Some(((total - done) as f64 / speed) as u64),
            entry_index: None,
            entry_count: None,
        }));
        if torrent {
            ctx.emit(JobEvent::Swarm(Swarm {
                peers: 8 + (rng.next_u64() % 40) as u32,
                seeds: 2 + (rng.next_u64() % 20) as u32,
                download_rate: speed,
                upload_rate: speed * rng.range(0.05, 0.4),
                uploaded: (done as f64 * 0.1) as u64,
                ratio: 0.1 * fraction,
            }));
        }
        if ticks.is_multiple_of(10) {
            ctx.emit(JobEvent::Files(file(done)));
        }
    }

    if polishes {
        ctx.phase(Status::Polishing, Some("merging audio and video"));
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs_f64(rng.range(1.0, 3.0) / engine.time_scale)) => {}
            _ = ctx.stop.stopped() => return Err(JobError::Stopped),
        }
    }

    if torrent {
        ctx.phase(Status::Seeding, None);
        let limit = request.options.seed_ratio_limit.unwrap_or(f64::INFINITY);
        let mut uploaded = (total as f64 * 0.1) as u64;
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = ctx.stop.stopped() => return Err(JobError::Stopped),
            }
            let upload = rate * rng.range(0.1, 0.6);
            uploaded += (upload * tick.as_secs_f64()) as u64;
            let ratio = uploaded as f64 / total as f64;
            ctx.emit(JobEvent::Swarm(Swarm {
                peers: 4 + (rng.next_u64() % 30) as u32,
                seeds: (rng.next_u64() % 12) as u32,
                download_rate: 0.0,
                upload_rate: upload,
                uploaded,
                ratio,
            }));
            if ratio >= limit {
                ctx.log(format!("Reached ratio {}; stopped seeding.", crate::format::ratio(limit)));
                break;
            }
        }
    }
    Ok(JobOutcome { files: vec![], polished: polishes as u32 })
}
