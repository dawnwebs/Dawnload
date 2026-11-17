//! Repeatable core memory probe, with the seeder in a separate process.
//! Run `torrent_memory seed DIR`, then `torrent_memory probe DIR 0|5`.
//! Measure the probe PID after its READY line. Fixtures and discovery stay on loopback.
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

use dawnload_core::engine::Lane;
use dawnload_core::engine::torrent::TorrentNetwork;
use dawnload_core::{Core, CoreConfig, Options, Status};
use librqbit::{CreateTorrentOptions, ListenerOptions, Session, SessionOptions};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Manifest {
    peer: SocketAddr,
    magnets: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let mode = args.get(1).map(String::as_str).unwrap_or("");
    let dir = PathBuf::from(args.get(2).ok_or_else(|| anyhow::anyhow!("usage: torrent_memory seed|probe DIR [0|5]"))?);
    if mode == "seed" {
        std::fs::create_dir_all(&dir)?;
        let session = Session::new_with_opts(
            dir.clone(),
            SessionOptions {
                dht: None,
                disable_trackers: true,
                disable_local_service_discovery: true,
                listen: Some(ListenerOptions { listen_addr: (Ipv4Addr::LOCALHOST, 0).into(), ..Default::default() }),
                ..Default::default()
            },
        )
        .await?;
        let mut magnets = Vec::new();
        for i in 0..5 {
            let path = dir.join(format!("fixture-{i}.bin"));
            std::fs::write(&path, vec![i as u8 + 1; 4 * 1024 * 1024])?;
            let (_, handle) = session
                .create_and_serve_torrent(&path, CreateTorrentOptions { piece_length: Some(64 * 1024), ..Default::default() })
                .await?;
            handle.wait_until_completed().await?;
            magnets.push(format!("magnet:?xt=urn:btih:{}", handle.info_hash().as_string()));
        }
        std::fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec(&Manifest { peer: session.listen_addr().unwrap(), magnets })?,
        )?;
        println!("READY seeder {}", std::process::id());
        tokio::signal::ctrl_c().await?;
        session.stop().await;
    } else if mode == "probe" {
        let count: usize = args.get(3).ok_or_else(|| anyhow::anyhow!("choose 0 or 5"))?.parse()?;
        anyhow::ensure!(count == 0 || count == 5, "choose 0 or 5");
        let manifest: Manifest = serde_json::from_slice(&std::fs::read(dir.join("manifest.json"))?)?;
        let state = dir.join(format!("probe-{count}"));
        let mut config = CoreConfig::new(state);
        config.check_tools = false;
        config.torrent_network =
            TorrentNetwork { offline: true, initial_peers: vec![manifest.peer], download_bps: Some((16 * 1024).try_into()?) };
        let core = Core::start(config)?;
        let mut settings = core.settings().await.unwrap();
        settings.lane_limits.insert(Lane::Torrent, 5);
        core.set_settings(settings).await;
        let mut ids = Vec::new();
        for (i, magnet) in manifest.magnets.iter().take(count).enumerate() {
            ids.push(
                core.add(
                    magnet,
                    Some(Options {
                        destination: dir.join(format!("download-{count}-{i}")),
                        seed_ratio_limit: None,
                        ..Default::default()
                    }),
                    true,
                )
                .await?,
            );
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                let mut ready = true;
                for id in &ids {
                    let item = core.detail(*id).await.unwrap().item;
                    assert_ne!(item.status, Status::Failed, "{:?}", item.failure);
                    ready &= item.transfer.bytes_done.is_some_and(|b| b > 0);
                }
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;
        println!("READY probe {} torrents={count}", std::process::id());
        tokio::signal::ctrl_c().await?;
        core.shutdown().await;
    } else {
        anyhow::bail!("choose seed or probe");
    }
    Ok(())
}
