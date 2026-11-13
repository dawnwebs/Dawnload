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
                // ProjectDirs nests it as …\Local\Dawn Labs\Dawnload\data; the plain name is
                // %LOCALAPPDATA%\Dawnload.
                directories::BaseDirs::new()
                    .map(|base| base.data_local_dir().join("Dawnload"))
                    .unwrap_or_else(|| dirs.data_local_dir().to_path_buf())
            } else {
                dirs.data_dir().to_path_buf()
            }
        })
        .unwrap_or_else(|| std::env::temp_dir().join("Dawnload"))
}

/// Before phase 3, Windows builds kept their data in `%LOCALAPPDATA%\Dawn Labs\Dawnload\Dawnload`.
/// Moves it to [`data_dir`] once, when the new folder doesn't exist yet, and says so. Call it before
/// anything opens the data folder.
pub fn migrate_legacy_data_dir() -> Option<String> {
    if !cfg!(windows) || std::env::var_os("DAWNLOAD_DATA_DIR").is_some() {
        return None;
    }
    let old = directories::ProjectDirs::from("app", "Dawn Labs", "Dawnload")?.data_local_dir().parent()?.join("Dawnload");
    move_data_dir(&old, &data_dir())
}

fn move_data_dir(old: &std::path::Path, new: &std::path::Path) -> Option<String> {
    if old == new || !old.is_dir() || new.exists() {
        return None;
    }
    std::fs::create_dir_all(new.parent()?).ok()?;
    match std::fs::rename(old, new) {
        Ok(()) => Some(format!("Moved Dawnload's data from {} to {}.", old.display(), new.display())),
        Err(e) => Some(format!("Couldn't move Dawnload's data from {} to {}: {e}", old.display(), new.display())),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_old_data_folder_moves_once() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Dawn Labs").join("Dawnload").join("Dawnload");
        std::fs::create_dir_all(old.join("tools")).unwrap();
        std::fs::write(old.join("dawnload.sqlite3"), b"db").unwrap();
        let new = dir.path().join("Dawnload");
        assert!(move_data_dir(&old, &new).unwrap().starts_with("Moved"));
        assert_eq!(std::fs::read(new.join("dawnload.sqlite3")).unwrap(), b"db");
        assert!(!old.exists());
        // Never over an existing folder, never twice.
        std::fs::create_dir_all(&old).unwrap();
        assert_eq!(move_data_dir(&old, &new), None);
    }
}
