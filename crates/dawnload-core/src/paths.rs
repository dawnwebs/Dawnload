//! Where Dawnload keeps things on each OS.
//!
//! - Data (the SQLite state, managed tools): `~/Library/Application Support/Dawnload` on macOS,
//!   `%LOCALAPPDATA%\Dawnload` on Windows, `$XDG_DATA_HOME/dawnload` on Linux.
//! - Downloads: `<Downloads>/Dawnload`.
//!
//! `DAWNLOAD_DATA_DIR` overrides the data folder (tests, portable installs).

use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("DAWNLOAD_DATA_DIR") {
        return PathBuf::from(dir);
    }
    directories::ProjectDirs::from("app", "Dawn Labs", "Dawnload")
        .map(|dirs| {
            // macOS: …/Application Support/app.Dawn-Labs.Dawnload is noisy; use the plain name.
            if cfg!(target_os = "macos") {
                dirs.data_dir().parent().map(|p| p.join("Dawnload")).unwrap_or_else(|| dirs.data_dir().to_path_buf())
            } else if cfg!(windows) {
                dirs.data_local_dir().parent().map(|p| p.join("Dawnload")).unwrap_or_else(|| dirs.data_local_dir().to_path_buf())
            } else {
                dirs.data_dir().to_path_buf()
            }
        })
        .unwrap_or_else(|| std::env::temp_dir().join("Dawnload"))
}

pub fn default_download_dir() -> PathBuf {
    let base = directories::UserDirs::new()
        .and_then(|dirs| dirs.download_dir().map(PathBuf::from).or_else(|| Some(dirs.home_dir().join("Downloads"))))
        .unwrap_or_else(std::env::temp_dir);
    base.join("Dawnload")
}

pub fn database_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("dawnload.sqlite3")
}

pub fn tools_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("tools")
}
