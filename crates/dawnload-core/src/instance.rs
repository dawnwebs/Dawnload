//! One running app per data folder, and a way for a second launch to hand its work to the first.
//!
//! The core already refuses to run twice on one data folder (`dawnload.lock`, ADR 0004). This adds
//! the friendly half: the first app listens on a local socket (a Unix socket in the data folder, or
//! a named pipe on Windows), and a second launch, say from a browser's "Open with" or a double-clicked
//! `.torrent`, sends what it was given (links, paths, "show the window") and exits.
//!
//! The protocol is one JSON line per connection. Only the same user can connect: the socket lives in
//! the user's own data folder, and Windows' default pipe security gives other users read access only.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;

/// What a second launch hands over.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Handoff {
    /// Links, magnets or `.torrent` paths to add, as given on the command line.
    pub links: Vec<String>,
    /// Bring the window to the front.
    pub show: bool,
}

/// Where the first app listens for `data_dir`.
pub fn endpoint(data_dir: &Path) -> PathBuf {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(data_dir.to_string_lossy().as_bytes());
    let tag = hex::encode(&digest[..8]);
    if cfg!(windows) {
        PathBuf::from(format!(r"\\.\pipe\dawnload-{tag}"))
    } else {
        let path = data_dir.join("dawnload.sock");
        // Unix socket paths are limited to ~104 bytes; a deep data folder gets one in the temp dir.
        if path.as_os_str().len() < 100 { path } else { std::env::temp_dir().join(format!("dawnload-{tag}.sock")) }
    }
}

/// Sends `handoff` to the app running on `data_dir`. Errors when nothing is listening.
pub async fn send(data_dir: &Path, handoff: &Handoff) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(handoff).map_err(std::io::Error::other)?;
    line.push(b'\n');
    let endpoint = endpoint(data_dir);
    #[cfg(unix)]
    {
        let mut stream = tokio::net::UnixStream::connect(&endpoint).await?;
        stream.write_all(&line).await?;
        stream.shutdown().await?;
    }
    #[cfg(windows)]
    {
        let mut client = tokio::net::windows::named_pipe::ClientOptions::new().open(&endpoint)?;
        client.write_all(&line).await?;
        client.flush().await?;
    }
    Ok(())
}

/// Listens for handoffs. Call only while holding the data folder's lock (the core does), so a
/// stale socket from a crash can be replaced safely. Handoffs arrive on the returned channel.
pub fn listen(data_dir: &Path) -> std::io::Result<mpsc::UnboundedReceiver<Handoff>> {
    let (tx, rx) = mpsc::unbounded_channel();
    let endpoint = endpoint(data_dir);
    #[cfg(unix)]
    {
        let _ = std::fs::remove_file(&endpoint);
        let listener = tokio::net::UnixListener::bind(&endpoint)?;
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let tx = tx.clone();
                tokio::spawn(async move { read_one(stream, tx).await });
            }
        });
    }
    #[cfg(windows)]
    {
        use tokio::net::windows::named_pipe::ServerOptions;
        let mut server = ServerOptions::new().first_pipe_instance(true).create(&endpoint)?;
        tokio::spawn(async move {
            loop {
                if server.connect().await.is_err() {
                    break;
                }
                let connected = server;
                server = match ServerOptions::new().create(&endpoint) {
                    Ok(next) => next,
                    Err(_) => break,
                };
                let tx = tx.clone();
                tokio::spawn(async move { read_one(connected, tx).await });
            }
        });
    }
    Ok(rx)
}

async fn read_one(stream: impl tokio::io::AsyncRead + Unpin, tx: mpsc::UnboundedSender<Handoff>) {
    let mut line = String::new();
    // One small line; anything longer than 1 MB isn't ours.
    let mut reader = BufReader::new(tokio::io::AsyncReadExt::take(stream, 1 << 20));
    if reader.read_line(&mut line).await.is_ok()
        && let Ok(handoff) = serde_json::from_str::<Handoff>(line.trim())
    {
        let _ = tx.send(handoff);
    }
}

/// The links in a command line: everything that isn't a flag.
pub fn links_from_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter().filter(|a| !a.starts_with("--") && !a.trim().is_empty()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_second_launch_hands_its_links_to_the_first() {
        let dir = tempfile::tempdir().unwrap();
        assert!(send(dir.path(), &Handoff::default()).await.is_err(), "nothing listens yet");
        let mut rx = listen(dir.path()).unwrap();
        let handoff = Handoff { links: vec!["https://vimeo.com/1".into(), "/tmp/a b.torrent".into()], show: true };
        send(dir.path(), &handoff).await.unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
        assert_eq!(got, handoff);
        // And again: the listener keeps going.
        send(dir.path(), &Handoff { links: vec![], show: true }).await.unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
        assert!(got.show && got.links.is_empty());
    }

    #[test]
    fn endpoints_differ_per_data_folder() {
        assert_ne!(endpoint(Path::new("/a")), endpoint(Path::new("/b")));
        assert_eq!(links_from_args(["--real".to_string(), "https://x.org/a".into(), " ".into()]), vec!["https://x.org/a"]);
    }
}
