//! Torrent seeding policy shared by the scheduler and the torrent engine.

use std::num::NonZeroU32;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::model::Options;

pub const DEFAULT_SEED_SECONDS: u64 = 7 * 24 * 60 * 60;
pub(crate) const MIN_UPLOAD_BPS: u32 = 16 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadCap {
    #[default]
    Auto,
    /// Bytes per second, with a floor of one BitTorrent request block.
    Fixed(NonZeroU32),
    Unlimited,
}

impl UploadCap {
    pub fn valid(self) -> bool {
        !matches!(self, Self::Fixed(bps) if bps.get() < MIN_UPLOAD_BPS)
    }

    pub(crate) fn limit(self, peak: u32) -> Option<NonZeroU32> {
        match self {
            Self::Auto if peak != 0 => NonZeroU32::new(((u64::from(peak) * 7 / 10) as u32).max(MIN_UPLOAD_BPS)),
            Self::Auto | Self::Unlimited => None,
            Self::Fixed(bps) => NonZeroU32::new(bps.get().max(MIN_UPLOAD_BPS)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SeedingSettings {
    pub courtesy: bool,
    pub upload_cap: UploadCap,
}

impl Default for SeedingSettings {
    fn default() -> Self {
        Self { courtesy: true, upload_cap: UploadCap::Auto }
    }
}

pub fn validate_ratio(ratio: Option<f64>) -> Result<(), &'static str> {
    if ratio.is_some_and(|ratio| !ratio.is_finite() || ratio < 0.0) {
        return Err("Enter a finite seed ratio of zero or more.");
    }
    Ok(())
}

/// Empty or "off" disables one limit. Both disabled means seeding until stopped.
pub fn parse_ratio(value: &str) -> Result<Option<f64>, &'static str> {
    let value = value.trim();
    let ratio = if value.is_empty() || value.eq_ignore_ascii_case("off") {
        None
    } else {
        Some(value.parse().map_err(|_| "Enter a finite seed ratio of zero or more.")?)
    };
    validate_ratio(ratio)?;
    Ok(ratio)
}

pub fn parse_seed_days(value: &str) -> Result<Option<u64>, &'static str> {
    let value = value.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("off") {
        return Ok(None);
    }
    value.parse::<u64>().ok().and_then(|days| days.checked_mul(86_400)).map(Some).ok_or("Enter a whole number of days.")
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeedProgress {
    pub uploaded: u64,
    /// First completion of a file selection, in Unix milliseconds.
    pub started_at: Option<i64>,
}

impl SeedProgress {
    pub(crate) fn ratio(self, selected_bytes: u64) -> f64 {
        if selected_bytes == 0 { 0.0 } else { self.uploaded as f64 / selected_bytes as f64 }
    }

    pub(crate) fn reached(self, options: &Options, selected_bytes: u64, now: i64) -> bool {
        options.seed_ratio_limit.is_some_and(|limit| self.ratio(selected_bytes) >= limit)
            || self
                .started_at
                .zip(options.seed_time_limit_secs)
                .is_some_and(|(start, seconds)| (now.saturating_sub(start).max(0) as u64) / 1000 >= seconds)
    }
}

pub trait SeedClock: Send + Sync {
    fn now_millis(&self) -> i64;
}

pub(crate) struct SystemClock;

impl SeedClock for SystemClock {
    fn now_millis(&self) -> i64 {
        crate::model::now_millis()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeedConditions {
    pub metered: bool,
    pub on_battery: bool,
}

impl SeedConditions {
    pub(crate) fn pause_reason(self) -> Option<&'static str> {
        match (self.metered, self.on_battery) {
            (true, true) => Some("Paused on battery and metered network"),
            (true, false) => Some("Paused on metered network"),
            (false, true) => Some("Paused on battery"),
            (false, false) => None,
        }
    }
}

/// Reads local system state. Sampling runs off the async executor and is shared by the session.
pub trait SeedConditionsSource: Send + Sync {
    fn sample(&self) -> SeedConditions;
}

/// The live librqbit counter starts over each time a handle is unpaused.
#[derive(Default)]
pub(crate) struct UploadAccounting {
    pub progress: SeedProgress,
    last: u64,
}

impl UploadAccounting {
    pub fn new(progress: SeedProgress) -> Self {
        Self { progress, last: 0 }
    }

    pub fn sample(&mut self, uploaded: u64) {
        self.progress.uploaded = self.progress.uploaded.saturating_add(uploaded.saturating_sub(self.last));
        self.last = self.last.max(uploaded);
    }

    pub fn reset_live_counter(&mut self) {
        self.last = 0;
    }
}

/// Measure session-wide upload over windows, as a burst from one torrent isn't uplink capacity.
#[derive(Default)]
pub(crate) struct UploadEstimate {
    pub peak: u32,
    window: Option<(Instant, u64)>,
    last_second: Option<(Instant, u64)>,
}

impl UploadEstimate {
    pub fn new(peak: u32) -> Self {
        Self { peak, window: None, last_second: None }
    }

    pub fn wants_sample(&self, now: Instant) -> bool {
        self.peak == 0 && self.last_second.is_none_or(|(at, _)| now.saturating_duration_since(at) >= Duration::from_secs(1))
    }

    pub fn sample(&mut self, now: Instant, uploaded: u64) {
        if self.peak != 0 {
            return; // Once limited, the measured rate cannot estimate headroom.
        }
        let Some((previous_time, previous_bytes)) = self.last_second else {
            self.window = Some((now, uploaded));
            self.last_second = Some((now, uploaded));
            return;
        };
        let interval = now.saturating_duration_since(previous_time);
        if interval < Duration::from_secs(1) {
            return;
        }
        self.last_second = Some((now, uploaded));
        // Discard idle or interrupted windows instead of treating a short burst as sustained.
        if uploaded <= previous_bytes || interval > Duration::from_secs(2) {
            self.window = Some((now, uploaded));
            return;
        }
        let (since, start) = *self.window.get_or_insert((previous_time, previous_bytes));
        let elapsed = now.saturating_duration_since(since);
        if elapsed >= Duration::from_secs(5) {
            let bps = uploaded.saturating_sub(start) as f64 / elapsed.as_secs_f64();
            self.peak = self.peak.max(bps as u32);
        }
    }

    pub fn reset_window(&mut self) {
        self.window = None;
        self.last_second = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetime_ratio_uses_the_current_file_selection_and_preserves_lifetime_bytes() {
        let options = Options::default();
        let mut bytes = UploadAccounting::new(SeedProgress { uploaded: 900, started_at: Some(1_000) });
        bytes.sample(50);
        bytes.sample(50);
        bytes.reset_live_counter();
        bytes.sample(25);
        assert_eq!(bytes.progress.uploaded, 975);
        assert!(!bytes.progress.reached(&options, 1_000, 1_000));
        assert!(bytes.progress.reached(&options, 900, 1_000));
        assert!(!bytes.progress.reached(&options, 1_100, 1_000));
    }

    #[test]
    fn auto_uses_a_sustained_window_and_learns_again_when_reset() {
        let start = Instant::now();
        let mut estimate = UploadEstimate::default();
        estimate.sample(start, 0);
        estimate.sample(start + Duration::from_secs(1), 500_000);
        for second in 2..=5 {
            estimate.sample(start + Duration::from_secs(second), 500_000);
        }
        assert_eq!(UploadCap::Auto.limit(estimate.peak), None);
        for second in 6..=10 {
            estimate.sample(start + Duration::from_secs(second), second * 100_000);
        }
        assert_eq!(UploadCap::Auto.limit(estimate.peak).unwrap().get(), 70_000);
        estimate = UploadEstimate::default();
        for second in 0..=5 {
            estimate.sample(start + Duration::from_secs(second), second * 200_000);
        }
        assert_eq!(UploadCap::Auto.limit(estimate.peak).unwrap().get(), 140_000);
    }
}
