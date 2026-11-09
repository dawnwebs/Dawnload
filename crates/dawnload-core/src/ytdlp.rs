//! Reading yt-dlp. Dawnload asks yt-dlp to print machine-readable lines (markers), so progress,
//! parts, polishing and the final file names never depend on yt-dlp's human output.

use serde::Deserialize;

use crate::filename::SmartFilenameMetadata;
use crate::model::Transfer;

pub const ITEM: &str = "__DAWNLOAD_ITEM__";
pub const PARTS: &str = "__DAWNLOAD_PARTS__";
pub const PROGRESS: &str = "__DAWNLOAD_PROGRESS__";
pub const POST: &str = "__DAWNLOAD_POST__";
pub const METADATA: &str = "__DAWNLOAD_METADATA__";

/// The yt-dlp arguments that make it print the lines [`YtDlpLine::parse`] reads.
pub fn marker_arguments() -> Vec<String> {
    vec![
        "--print".into(),
        format!("before_dl:{ITEM}%(.{{id,title,playlist_index,n_entries,playlist_title,format_id,filesize,filesize_approx}})j"),
        "--print".into(),
        format!("before_dl:{PARTS}%(requested_formats.:.{{format_id,filesize,filesize_approx}})j"),
        "--progress-template".into(),
        format!(
            "download:{PROGRESS}%(progress.downloaded_bytes)s %(progress.total_bytes)s %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s %(info.format_id)s %(progress.status)s"
        ),
        "--progress-template".into(),
        format!("postprocess:{POST}%(progress.status)s %(progress.postprocessor)s"),
        "--print".into(),
        format!("after_move:{METADATA}%(.{{id,title,fulltitle,display_id,uploader,upload_date,webpage_url,filepath}})j"),
    ]
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ItemInfo {
    pub id: Option<String>,
    pub title: Option<String>,
    pub playlist_index: Option<u32>,
    #[serde(rename = "n_entries")]
    pub entry_count: Option<u32>,
    pub playlist_title: Option<String>,
    pub format_id: Option<String>,
    pub filesize: Option<f64>,
    pub filesize_approx: Option<f64>,
}

impl ItemInfo {
    pub fn expected_size(&self) -> Option<u64> {
        self.filesize.or(self.filesize_approx).map(|v| v.max(0.0) as u64)
    }

    /// The item's title: the playlist's name for a playlist, the video's otherwise.
    pub fn item_title(&self) -> Option<&str> {
        if self.entry_count.is_some_and(|n| n > 1) {
            if let Some(name) = self.playlist_title.as_deref().filter(|t| !t.is_empty()) {
                return Some(name);
            }
        }
        self.title.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Part {
    pub format_id: String,
    pub filesize: Option<f64>,
    pub filesize_approx: Option<f64>,
}

impl Part {
    pub fn expected_size(&self) -> Option<u64> {
        self.filesize.or(self.filesize_approx).map(|v| v.max(0.0) as u64)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProgressLine {
    pub downloaded: Option<u64>,
    pub total: Option<u64>,
    pub total_estimate: Option<u64>,
    pub speed: Option<f64>,
    pub eta: Option<u64>,
    pub format_id: Option<String>,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum YtDlpLine {
    Item(ItemInfo),
    Parts(Vec<Part>),
    Progress(ProgressLine),
    /// A postprocessor started or finished. `is_polish` is false for yt-dlp's own file moves.
    Postprocess { started: bool, name: String, is_polish: bool },
    Metadata(Box<SmartFilenameMetadata>),
    Text(String),
}

fn payload<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    line.find(marker).map(|i| line[i + marker.len()..].trim())
}

fn number(field: &str) -> Option<u64> {
    if let Ok(v) = field.parse::<u64>() {
        return Some(v);
    }
    field.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).map(|v| v as u64)
}

impl YtDlpLine {
    pub fn parse(line: &str) -> YtDlpLine {
        if let Some(p) = payload(line, PROGRESS) {
            let fields: Vec<&str> = p.split_whitespace().collect();
            if fields.len() < 7 {
                return YtDlpLine::Text(line.to_string());
            }
            return YtDlpLine::Progress(ProgressLine {
                downloaded: number(fields[0]),
                total: number(fields[1]),
                total_estimate: number(fields[2]),
                speed: fields[3].parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0),
                eta: fields[4].parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0).map(|v| v.round() as u64),
                format_id: (fields[5] != "NA").then(|| fields[5].to_string()),
                finished: fields[6] == "finished",
            });
        }
        if let Some(p) = payload(line, ITEM) {
            return match serde_json::from_str::<ItemInfo>(p) {
                Ok(info) => YtDlpLine::Item(info),
                Err(_) => YtDlpLine::Text(line.to_string()),
            };
        }
        if let Some(p) = payload(line, PARTS) {
            return YtDlpLine::Parts(serde_json::from_str(p).unwrap_or_default());
        }
        if let Some(p) = payload(line, POST) {
            let Some((status, name)) = p.split_once(' ') else { return YtDlpLine::Text(line.to_string()) };
            return YtDlpLine::Postprocess { started: status == "started", name: name.to_string(), is_polish: name != "MoveFiles" };
        }
        if let Some(p) = payload(line, METADATA) {
            return match serde_json::from_str::<SmartFilenameMetadata>(p) {
                Ok(m) => YtDlpLine::Metadata(Box::new(m)),
                Err(_) => YtDlpLine::Text(line.to_string()),
            };
        }
        YtDlpLine::Text(line.to_string())
    }
}

/// Words for a postprocessor, for "Polishing · merging audio and video".
pub fn polish_description(postprocessor: &str) -> &'static str {
    match postprocessor {
        "Merger" => "merging audio and video",
        "VideoRemuxer" | "VideoConvertor" => "remuxing to MP4",
        "ExtractAudio" => "extracting audio",
        "Metadata" | "FFmpegMetadata" => "writing metadata",
        "EmbedThumbnail" => "embedding the thumbnail",
        "FixupM3u8" | "FixupM4a" | "FixupStretched" | "FixupDuplicateMoov" | "FixupTimestamp" | "FixupDuration" => {
            "fixing the container"
        }
        _ => "finishing with ffmpeg",
    }
}

/// Every metadata record in a run's output, for smart filenames.
pub fn metadata_records(output: &str) -> Vec<SmartFilenameMetadata> {
    output
        .lines()
        .filter_map(|line| match YtDlpLine::parse(line) {
            YtDlpLine::Metadata(m) => Some(*m),
            _ => None,
        })
        .collect()
}

/// Turns yt-dlp's per-part, per-entry progress into one number for the item.
#[derive(Clone, Debug, Default)]
pub struct ProgressTracker {
    pub title: Option<String>,
    entry_index: Option<u32>,
    entry_count: Option<u32>,
    entry_size: Option<u64>,
    part_order: Vec<String>,
    part_sizes: std::collections::HashMap<String, u64>,
    finished_bytes: std::collections::HashMap<String, u64>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one parsed line. Returns a sample when the line moved progress.
    pub fn consume(&mut self, line: &YtDlpLine) -> Option<Transfer> {
        match line {
            YtDlpLine::Item(info) => {
                if info.title.is_some() {
                    self.title = info.title.clone();
                }
                self.entry_index = info.playlist_index;
                self.entry_count = info.entry_count;
                self.entry_size = info.expected_size();
                self.part_order.clear();
                self.part_sizes.clear();
                self.finished_bytes.clear();
                None
            }
            YtDlpLine::Parts(parts) => {
                self.part_order = parts.iter().map(|p| p.format_id.clone()).collect();
                for part in parts {
                    if let Some(size) = part.expected_size() {
                        self.part_sizes.insert(part.format_id.clone(), size);
                    }
                }
                None
            }
            YtDlpLine::Progress(progress) => Some(self.sample(progress)),
            _ => None,
        }
    }

    fn sample(&mut self, progress: &ProgressLine) -> Transfer {
        let key = progress.format_id.clone().unwrap_or_else(|| "_".into());
        let current_total = progress.total.or(progress.total_estimate).or_else(|| self.part_sizes.get(&key).copied());
        if let Some(t) = current_total {
            if !self.part_sizes.contains_key(&key) || progress.total.is_some() {
                self.part_sizes.insert(key.clone(), t);
            }
        }
        if !self.part_order.is_empty() && !self.part_order.contains(&key) {
            self.part_order.push(key.clone());
        }

        let current = progress.downloaded.unwrap_or(0);
        if progress.finished {
            self.finished_bytes.insert(key.clone(), current.max(current_total.unwrap_or(current)));
        }

        let mut done = None;
        let mut total = None;
        if self.part_order.len() > 1 {
            let sizes: Vec<u64> = self.part_order.iter().filter_map(|p| self.part_sizes.get(p).copied()).collect();
            if sizes.len() == self.part_order.len() {
                total = Some(sizes.iter().sum());
                let finished: u64 = self.finished_bytes.iter().filter(|(k, _)| **k != key).map(|(_, v)| *v).sum();
                let this = if progress.finished { self.finished_bytes.get(&key).copied().unwrap_or(current) } else { current };
                done = Some(finished + this);
            }
        } else {
            total = current_total.or(self.entry_size);
            done = Some(if progress.finished { self.finished_bytes.get(&key).copied().unwrap_or(current) } else { current });
        }

        let mut entry_fraction = None;
        if let (Some(d), Some(t)) = (done, total) {
            if t > 0 {
                entry_fraction = Some((d as f64 / t as f64).clamp(0.0, 1.0));
            }
        } else if self.part_order.len() > 1 {
            // Sizes unknown for some parts: weight parts equally.
            if let (Some(t), Some(index)) = (current_total, self.part_order.iter().position(|p| *p == key)) {
                if t > 0 {
                    let part = (current as f64 / t as f64).min(1.0);
                    entry_fraction = Some((index as f64 + part) / self.part_order.len() as f64);
                }
            }
        }

        let mut fraction = entry_fraction;
        if let (Some(index), Some(count)) = (self.entry_index, self.entry_count) {
            if count > 1 {
                fraction = Some((index.saturating_sub(1) as f64 + entry_fraction.unwrap_or(0.0)) / count as f64);
            }
        }

        let mut seconds_left = progress.eta;
        if let (Some(speed), Some(d), Some(t)) = (progress.speed, done, total) {
            if speed > 0.0 && t >= d {
                seconds_left = Some(((t - d) as f64 / speed).round() as u64);
            }
        }

        Transfer {
            fraction,
            bytes_done: done,
            bytes_total: total,
            bytes_per_second: progress.speed,
            seconds_left: if progress.finished { None } else { seconds_left },
            entry_index: self.entry_index,
            entry_count: self.entry_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Lines captured from yt-dlp 2026.07.04 with the marker arguments.
    const MERGED_RUN: [&str; 7] = [
        r#"__DAWNLOAD_ITEM__{"id": "jNQXAC9IVRw", "title": "Me at the zoo", "format_id": "395+251", "filesize_approx": 475961}"#,
        r#"__DAWNLOAD_PARTS__[{"format_id": "395", "filesize": 223779, "filesize_approx": 223778}, {"format_id": "251", "filesize": 252182, "filesize_approx": 252180}]"#,
        "__DAWNLOAD_PROGRESS__1024 223779 NA 616119.25 0 395 downloading",
        "__DAWNLOAD_PROGRESS__223779 223779 NA 1742554.12 NA 395 finished",
        "__DAWNLOAD_PROGRESS__126091 252182 NA 1816425.13 0 251 downloading",
        "__DAWNLOAD_PROGRESS__252182 252182 NA 1816425.13 NA 251 finished",
        "__DAWNLOAD_POST__started Merger",
    ];

    #[test]
    fn parses_each_marker() {
        let YtDlpLine::Item(info) = YtDlpLine::parse(MERGED_RUN[0]) else { panic!() };
        assert_eq!(info.id.as_deref(), Some("jNQXAC9IVRw"));
        assert_eq!(info.expected_size(), Some(475_961));
        let YtDlpLine::Parts(parts) = YtDlpLine::parse(MERGED_RUN[1]) else { panic!() };
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1].expected_size(), Some(252_182));
        assert_eq!(
            YtDlpLine::parse(MERGED_RUN[2]),
            YtDlpLine::Progress(ProgressLine {
                downloaded: Some(1024),
                total: Some(223_779),
                total_estimate: None,
                speed: Some(616_119.25),
                eta: Some(0),
                format_id: Some("395".into()),
                finished: false,
            })
        );
        assert_eq!(
            YtDlpLine::parse(MERGED_RUN[6]),
            YtDlpLine::Postprocess { started: true, name: "Merger".into(), is_polish: true }
        );
        assert_eq!(
            YtDlpLine::parse("__DAWNLOAD_POST__started MoveFiles"),
            YtDlpLine::Postprocess { started: true, name: "MoveFiles".into(), is_polish: false }
        );
        assert_eq!(YtDlpLine::parse("__DAWNLOAD_PARTS__NA"), YtDlpLine::Parts(vec![]));
        assert_eq!(YtDlpLine::parse("[youtube] Extracting URL"), YtDlpLine::Text("[youtube] Extracting URL".into()));
        assert_eq!(polish_description("Merger"), "merging audio and video");
    }

    #[test]
    fn progress_adds_up_across_video_and_audio_parts() {
        let mut tracker = ProgressTracker::new();
        let samples: Vec<Transfer> = MERGED_RUN.iter().filter_map(|l| tracker.consume(&YtDlpLine::parse(l))).collect();
        assert_eq!(tracker.title.as_deref(), Some("Me at the zoo"));
        assert_eq!(samples.len(), 4);
        let total = 223_779 + 252_182;
        assert_eq!(samples[0].bytes_total, Some(total));
        assert_eq!(samples[0].bytes_done, Some(1024));
        assert!((samples[1].fraction.unwrap() - 223_779.0 / total as f64).abs() < 1e-4);
        // The audio part continues from where the video left off; the fill never goes backwards.
        assert_eq!(samples[2].bytes_done, Some(223_779 + 126_091));
        assert!(samples[2].fraction.unwrap() > samples[1].fraction.unwrap());
        assert_eq!(samples[3].fraction, Some(1.0));
        assert_eq!(samples[3].seconds_left, None);
    }

    #[test]
    fn single_format_with_estimate_only() {
        let mut tracker = ProgressTracker::new();
        tracker.consume(&YtDlpLine::parse(r#"__DAWNLOAD_ITEM__{"title": "Clip", "format_id": "18"}"#));
        tracker.consume(&YtDlpLine::parse("__DAWNLOAD_PARTS__NA"));
        let sample = tracker.consume(&YtDlpLine::parse("__DAWNLOAD_PROGRESS__500 NA 1000.0 100.0 NA 18 downloading")).unwrap();
        assert_eq!(sample.fraction, Some(0.5));
        assert_eq!(sample.bytes_total, Some(1000));
        assert_eq!(sample.seconds_left, Some(5));
    }

    #[test]
    fn unknown_size_has_no_fraction() {
        let mut tracker = ProgressTracker::new();
        let sample = tracker.consume(&YtDlpLine::parse("__DAWNLOAD_PROGRESS__500 NA NA NA NA 18 downloading")).unwrap();
        assert_eq!(sample.fraction, None);
        assert_eq!(sample.bytes_done, Some(500));
        assert_eq!(sample.bytes_per_second, None);
    }

    #[test]
    fn playlist_progress_counts_entries() {
        let mut tracker = ProgressTracker::new();
        let line = YtDlpLine::parse(
            r#"__DAWNLOAD_ITEM__{"title": "Part 3", "playlist_index": 3, "n_entries": 4, "playlist_title": "Open Movies", "format_id": "18", "filesize": 100}"#,
        );
        let YtDlpLine::Item(info) = &line else { panic!() };
        assert_eq!(info.item_title(), Some("Open Movies"));
        tracker.consume(&line);
        let sample = tracker.consume(&YtDlpLine::parse("__DAWNLOAD_PROGRESS__50 100 NA 10 NA 18 downloading")).unwrap();
        assert!((sample.fraction.unwrap() - 2.5 / 4.0).abs() < 1e-4);
        assert_eq!(sample.entry_index, Some(3));
        assert_eq!(sample.entry_count, Some(4));
    }

    #[test]
    fn metadata_from_mixed_output() {
        let output = "[download] 100% of 1.00MiB\n__DAWNLOAD_METADATA__{\"id\":\"77ii8\",\"title\":\"77ii8\",\"webpage_url\":\"https://spankbang.com/77ii8/video/fc2+ppv\",\"filepath\":\"/tmp/77ii8.mp4\"}\n[Merger] Finished\n__DAWNLOAD_METADATA__not-json";
        let records = metadata_records(output);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id.as_deref(), Some("77ii8"));
        assert_eq!(records[0].filepath.as_deref(), Some("/tmp/77ii8.mp4"));
    }
}
