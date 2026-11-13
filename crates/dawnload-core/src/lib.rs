//! Dawnload's core: everything except pixels.
//!
//! - [`source`]: what was pasted (link, magnet, .torrent) and which engine takes it.
//! - [`engine`]: the engines (yt-dlp, gallery-dl, built-in HTTP, torrent seam, demo).
//! - [`tools`]: finding, installing, verifying and updating yt-dlp, gallery-dl, ffmpeg and deno.
//! - [`core`]: the queue and state owner; UIs talk to it through [`core::CoreHandle`].
//! - [`api`]: snapshots, batched diffs and the [`api::Mirror`] every UI keeps.
//! - [`store`]: SQLite persistence.

pub mod api;
pub mod core;
pub mod engine;
pub mod explain;
pub mod failure;
pub mod filename;
pub mod finishing;
pub mod format;
pub mod instance;
pub mod intake;
pub mod model;
pub mod net;
pub mod paths;
pub mod present;
pub mod process;
pub mod source;
pub mod store;
pub mod tools;
pub mod ytdlp;

pub use crate::api::{Action, Change, ItemView, ListFilter, Mirror, Row, Section, Snapshot, Update};
pub use crate::core::{AppPrefs, Appearance, Core, CoreConfig, CoreError, CoreHandle, Motion, Settings, Subscription};
pub use crate::failure::{DownloadFailure, FailureKind};
pub use crate::model::{Item, ItemId, Options, Quality, Status};
pub use crate::source::{DownloadSource, EngineId, EnginePreference};
