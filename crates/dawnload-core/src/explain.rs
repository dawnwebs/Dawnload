//! The detection line under the paste field: what will happen, in plain words, before anything
//! happens. "youtube.com · yt-dlp will download this (Auto)".

use serde::{Deserialize, Serialize};

use crate::engine::{Availability, Registry};
use crate::model::Options;
use crate::source::{DownloadSource, EngineId, EnginePreference, is_youtube};
use crate::tools::ToolSnapshot;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
    Neutral,
    Warning,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Explanation {
    pub text: String,
    pub tone: Tone,
    pub source: Option<DownloadSource>,
    pub engine: Option<EngineId>,
    /// Download is on. False when what was pasted can't be downloaded here yet.
    pub can_submit: bool,
}

/// `None` for an empty field.
pub fn explain(
    text: &str,
    options: &Options,
    tools: &ToolSnapshot,
    registry: &Registry,
    engine_override: Option<EngineId>,
) -> Option<Explanation> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let Some(source) = DownloadSource::parse(trimmed) else {
        return Some(Explanation {
            text: "A full link starts with https://.".into(),
            tone: Tone::Neutral,
            source: None,
            engine: None,
            can_submit: true,
        });
    };
    let engine_id = engine_override.unwrap_or_else(|| options.engine.resolve(&source));
    let availability = registry.get(engine_id).map(|e| e.availability(tools, options)).unwrap_or(Availability::NotBuilt);
    let auto = if options.engine == EnginePreference::Auto { " (Auto)" } else { "" };

    let (text, tone, can_submit) = match (&source, &availability) {
        (_, Availability::NotBuilt) => {
            let what = match &source {
                DownloadSource::Magnet(m) => match &m.display_name {
                    Some(name) => format!("Magnet link · {name}"),
                    None => "Magnet link".to_string(),
                },
                DownloadSource::TorrentFile { .. } => format!(".torrent file · {}", source.short_name()),
                DownloadSource::Web { .. } => format!("{} · .torrent link", source.short_name()),
            };
            (format!("{what} · Torrent downloads aren't built yet."), Tone::Warning, false)
        }
        (_, Availability::MissingTool(_)) => {
            let failure = availability.failure().expect("missing tool has a failure");
            (format!("{} · {}", source.short_name(), failure.sentence()), Tone::Warning, false)
        }
        (DownloadSource::Web { url }, Availability::Ready) => {
            let host = source.short_name();
            match engine_id {
                EngineId::Direct => {
                    let name = url::Url::parse(url)
                        .ok()
                        .and_then(|u| u.path_segments().and_then(|mut s| s.next_back()).map(|s| percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()))
                        .filter(|n| !n.is_empty());
                    let file = name.map(|n| format!(" · {n}")).unwrap_or_default();
                    let connections = options.connections.max(1);
                    let how = if connections > 1 { format!(" over {connections} connections") } else { String::new() };
                    (format!("{host}{file} · Dawnload will download this file directly{how}{auto}"), Tone::Neutral, true)
                }
                EngineId::YtDlp if is_youtube(&source) && tools.js_runtime.is_none() => (
                    format!("{host} · yt-dlp will download this{auto}. YouTube needs a JavaScript runtime: install deno in Settings › Tools."),
                    Tone::Warning,
                    true,
                ),
                _ => (format!("{host} · {} will download this{auto}", engine_id.display_name()), Tone::Neutral, true),
            }
        }
        (_, Availability::Ready) => (format!("{} · {} will download this", source.short_name(), engine_id.display_name()), Tone::Neutral, true),
    };
    Some(Explanation { text, tone, source: Some(source), engine: Some(engine_id), can_submit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{JsRuntime, JsRuntimeKind, ToolId};

    fn tools(all: bool) -> ToolSnapshot {
        let mut t = ToolSnapshot::default();
        if all {
            for tool in ToolId::ALL {
                t.paths.insert(tool, format!("/bin/{}", tool.name()).into());
            }
            t.js_runtime = Some(JsRuntime { kind: JsRuntimeKind::Deno, path: "/bin/deno".into() });
        }
        t
    }

    fn say(text: &str, tools: &ToolSnapshot) -> Option<Explanation> {
        explain(text, &Options::default(), tools, &Registry::standard(), None)
    }

    #[test]
    fn explains_what_will_happen() {
        assert_eq!(say("   ", &tools(true)), None);
        let e = say("youtube.com/watch", &tools(true)).unwrap();
        assert_eq!((e.text.as_str(), e.can_submit), ("A full link starts with https://.", true));

        let e = say("https://www.youtube.com/watch?v=x", &tools(true)).unwrap();
        assert_eq!(e.text, "youtube.com · yt-dlp will download this (Auto)");
        assert_eq!(e.tone, Tone::Neutral);

        let mut no_js = tools(true);
        no_js.js_runtime = None;
        let e = say("https://youtu.be/x", &no_js).unwrap();
        assert_eq!(e.tone, Tone::Warning);
        assert!(e.text.contains("YouTube needs a JavaScript runtime"));
        assert!(e.can_submit);

        let e = say("https://www.instagram.com/p/x/", &tools(true)).unwrap();
        assert_eq!(e.text, "instagram.com · gallery-dl will download this (Auto)");

        let e = say("https://cdimage.example.org/releases/debian-13.iso", &tools(false)).unwrap();
        assert_eq!(e.text, "cdimage.example.org · debian-13.iso · Dawnload will download this file directly over 4 connections (Auto)");
        assert!(e.can_submit);

        let e = say("https://vimeo.com/1", &tools(false)).unwrap();
        assert_eq!(e.text, "vimeo.com · yt-dlp isn't installed. Dawnload can install it for you: Settings › Tools, or `dawnload tools install`.");
        assert!(!e.can_submit);

        let e = say("magnet:?xt=urn:btih:c9e15763f722f23e98a29decdfae341b98d53056&dn=Ubuntu+24.04", &tools(true)).unwrap();
        assert_eq!(e.text, "Magnet link · Ubuntu 24.04 · Torrent downloads aren't built yet.");
        assert!(!e.can_submit);
        assert_eq!(e.engine, Some(EngineId::Torrent));

        let e = say("/tmp/debian.torrent", &tools(true)).unwrap();
        assert_eq!(e.text, ".torrent file · debian.torrent · Torrent downloads aren't built yet.");
    }
}
