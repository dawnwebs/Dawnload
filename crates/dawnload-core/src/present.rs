//! The words on a row, the same in every UI. See docs/DESIGN.md › The list.

use crate::api::ItemView;
use crate::format;
use crate::model::Status;
use crate::source::SourceKind;

/// The mono line under a row's title.
pub fn subline(item: &ItemView, queue_position: Option<usize>) -> String {
    let t = &item.transfer;
    match item.status {
        Status::Queued => {
            if let Some(detail) = &item.phase_detail {
                return format!("{detail} · {}", item.host);
            }
            let position = format::queue_position(queue_position.unwrap_or(1));
            let mut parts = vec![position, item.host.clone()];
            if !item.options_summary.is_empty() {
                parts.push(item.options_summary.clone());
            }
            parts.join(" · ")
        }
        Status::Pulling => {
            if let (Some(index), Some(count)) = (t.entry_index, t.entry_count)
                && count > 1
            {
                let mut parts = vec![format!("Video {index} of {count}")];
                if let Some(speed) = t.bytes_per_second {
                    parts.push(format::speed(speed));
                }
                return parts.join(" · ");
            }
            let mut parts = Vec::new();
            if let Some(fraction) = t.fraction {
                parts.push(format::percent(fraction));
            }
            match (t.bytes_done, t.bytes_total) {
                (Some(done), Some(total)) => parts.push(format::bytes_of(done, total)),
                (Some(done), None) if done > 0 => parts.push(format::bytes(done)),
                _ => {}
            }
            if let Some(speed) = t.bytes_per_second {
                parts.push(format::speed(speed));
            }
            if let Some(left) = t.seconds_left {
                parts.push(format::time_left(left));
            }
            if item.source_kind != SourceKind::Web
                && let Some(swarm) = &item.swarm
            {
                parts.push(format::count(swarm.peers as usize, "peer"));
            }
            if parts.is_empty() {
                return item.phase_detail.clone().map(capitalize).unwrap_or_else(|| "Starting".into());
            }
            if let Some(detail) = &item.phase_detail {
                parts.push(detail.clone());
            }
            parts.join(" · ")
        }
        Status::Polishing => match &item.phase_detail {
            Some(detail) => format!("Polishing · {detail}"),
            None => "Polishing".into(),
        },
        Status::Kept => {
            let mut parts = vec![format!("Kept in {}", item.folder_name)];
            if item.file_count > 0 {
                parts.push(format::count(item.file_count, "file"));
            }
            if let Some(bytes) = item.kept_bytes {
                parts.push(format::bytes(bytes));
            }
            parts.join(" · ")
        }
        Status::Failed => item.failure.as_ref().map(|f| f.message.clone()).unwrap_or_else(|| "Failed.".into()),
        Status::Paused => match (t.fraction, t.bytes_done, t.bytes_total) {
            (Some(f), Some(done), Some(total)) => format!("Paused at {} · {}", format::percent(f), format::bytes_of(done, total)),
            (Some(f), _, _) => format!("Paused at {}", format::percent(f)),
            _ => "Paused".into(),
        },
        Status::Seeding => {
            let mut parts = vec!["Seeding".to_string()];
            if let Some(swarm) = &item.swarm {
                parts.push(format!("↑ {}", format::speed(swarm.upload_rate)));
                parts.push(format!("ratio {}", format::ratio(swarm.ratio)));
                parts.push(format::count(swarm.peers as usize, "peer"));
            }
            parts.join(" · ")
        }
    }
}

/// The trailing text on a row: the percent while it pulls.
pub fn trailing(item: &ItemView) -> String {
    match (item.status, item.transfer.fraction) {
        (Status::Pulling, Some(f)) => format::percent(f),
        _ => String::new(),
    }
}

/// One sentence for screen readers: "Big Buck Bunny, Pulling, 62 percent, 15 seconds left".
pub fn accessible_label(item: &ItemView) -> String {
    let mut parts = vec![item.title.clone(), item.status.word().to_string()];
    if item.status == Status::Pulling {
        if let Some(f) = item.transfer.fraction {
            parts.push(format!("{} percent", (f * 100.0).floor() as u32));
        }
        if let Some(s) = item.transfer.seconds_left {
            parts.push(format!("{s} seconds left"));
        }
    }
    if let Some(failure) = &item.failure {
        parts.push(failure.message.clone());
    }
    parts.join(", ")
}

fn capitalize(text: String) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ItemView;
    use crate::model::{Item, ItemId, Options, Swarm, Transfer};
    use crate::source::DownloadSource;

    fn view(status: Status) -> ItemView {
        let mut item = Item::new(
            ItemId(1),
            DownloadSource::parse("https://vimeo.com/1").unwrap(),
            Options { destination: "/x/Dawnload".into(), ..Options::default() },
        );
        item.status = status;
        ItemView::of(&item)
    }

    #[test]
    fn sublines_match_the_design() {
        assert_eq!(subline(&view(Status::Queued), Some(1)), "Up next · vimeo.com · Best · Polish");
        assert_eq!(subline(&view(Status::Queued), Some(3)), "3rd in line · vimeo.com · Best · Polish");

        let mut pulling = view(Status::Pulling);
        pulling.transfer = Transfer {
            fraction: Some(0.62),
            bytes_done: Some(28_300_000),
            bytes_total: Some(45_600_000),
            bytes_per_second: Some(2_300_000.0),
            seconds_left: Some(15),
            ..Transfer::default()
        };
        assert_eq!(subline(&pulling, None), "62% · 28.3 of 45.6 MB · 2.3 MB/s · 0:15 left");
        assert_eq!(trailing(&pulling), "62%");
        assert_eq!(accessible_label(&pulling), "vimeo.com/1, Pulling, 62 percent, 15 seconds left");

        pulling.transfer.entry_index = Some(3);
        pulling.transfer.entry_count = Some(12);
        assert_eq!(subline(&pulling, None), "Video 3 of 12 · 2.3 MB/s");

        let mut polishing = view(Status::Polishing);
        polishing.phase_detail = Some("merging audio and video".into());
        assert_eq!(subline(&polishing, None), "Polishing · merging audio and video");

        let mut kept = view(Status::Kept);
        kept.file_count = 1;
        kept.kept_bytes = Some(45_600_000);
        assert_eq!(subline(&kept, None), "Kept in Dawnload · 1 file · 45.6 MB");

        let mut paused = view(Status::Paused);
        paused.transfer =
            Transfer { fraction: Some(0.62), bytes_done: Some(28_300_000), bytes_total: Some(45_600_000), ..Transfer::default() };
        assert_eq!(subline(&paused, None), "Paused at 62% · 28.3 of 45.6 MB");

        let mut seeding = view(Status::Seeding);
        seeding.swarm = Some(Swarm { peers: 12, upload_rate: 1_200_000.0, ratio: 0.84, ..Swarm::default() });
        assert_eq!(subline(&seeding, None), "Seeding · ↑ 1.2 MB/s · ratio 0.84 · 12 peers");
    }
}
