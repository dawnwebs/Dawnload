//! Numbers in the brand's voice: short, specific, unit last. Every UI (the app, the CLI, a remote)
//! formats with these, so "28.3 of 45.6 MB" reads the same everywhere.

/// Decimal units, like the file sizes the OS shows: "900 KB", "45.6 MB", "1.23 GB".
pub fn bytes(value: u64) -> String {
    let v = value as f64;
    if v < 1_000_000.0 {
        format!("{} KB", (v / 1_000.0).round() as u64)
    } else if v < 1_000_000_000.0 {
        format!("{:.1} MB", v / 1_000_000.0)
    } else if v < 1_000_000_000_000.0 {
        format!("{:.2} GB", v / 1_000_000_000.0)
    } else {
        format!("{:.2} TB", v / 1_000_000_000_000.0)
    }
}

/// "28.3 of 45.6 MB", or "900 KB of 45.6 MB" when the units differ.
pub fn bytes_of(done: u64, total: u64) -> String {
    let done_text = bytes(done);
    let total_text = bytes(total);
    match (done_text.split_once(' '), total_text.split_once(' ')) {
        (Some((number, unit)), Some((_, total_unit))) if unit == total_unit => format!("{number} of {total_text}"),
        _ => format!("{done_text} of {total_text}"),
    }
}

pub fn speed(bytes_per_second: f64) -> String {
    format!("{}/s", bytes(bytes_per_second.max(0.0).round() as u64))
}

/// "0:15 left", "12:04 left", "1:02:10 left".
pub fn time_left(seconds: u64) -> String {
    format!("{} left", clock(seconds))
}

pub fn clock(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 { format!("{hours}:{minutes:02}:{secs:02}") } else { format!("{minutes}:{secs:02}") }
}

/// Rounded down, so 99.9% never reads as 100% before it is.
pub fn percent(fraction: f64) -> String {
    format!("{}%", (fraction.clamp(0.0, 1.0) * 100.0).floor() as u32)
}

/// Position among waiting items: "Up next", "2nd in line".
pub fn queue_position(index: usize) -> String {
    if index <= 1 { "Up next".to_string() } else { format!("{} in line", ordinal(index)) }
}

pub fn ordinal(number: usize) -> String {
    let suffix = match (number % 100, number % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{number}{suffix}")
}

pub fn ratio(value: f64) -> String {
    format!("{value:.2}")
}

/// "1 file", "3 files".
pub fn count(value: usize, singular: &str) -> String {
    if value == 1 { format!("1 {singular}") } else { format!("{value} {singular}s") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_the_swift_app() {
        assert_eq!(time_left(15), "0:15 left");
        assert_eq!(clock(3730), "1:02:10");
        assert_eq!(percent(0.629), "62%");
        assert_eq!(percent(0.9999), "99%");
        assert_eq!(queue_position(1), "Up next");
        assert_eq!(queue_position(2), "2nd in line");
        assert_eq!(queue_position(3), "3rd in line");
        assert_eq!(queue_position(11), "11th in line");
        assert_eq!(queue_position(22), "22nd in line");
        assert_eq!(count(1, "file"), "1 file");
        assert_eq!(count(3, "file"), "3 files");
        assert_eq!(bytes_of(28_300_000, 45_600_000), "28.3 of 45.6 MB");
        assert_eq!(bytes_of(900_000, 45_600_000), "900 KB of 45.6 MB");
        assert_eq!(bytes(6_114_770_944), "6.11 GB");
        assert_eq!(speed(2_300_000.0), "2.3 MB/s");
    }
}
