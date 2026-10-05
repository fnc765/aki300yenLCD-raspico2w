//! Matter power configuration and sample freshness, independent of hardware.
use core::fmt::Write as _;
use heapless::String;

pub const CONFIG_MAX: usize = 256;
pub const STALE_MS: u32 = 15_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerDisplay { Normal, Large, Graph }

impl PowerDisplay {
    pub fn parse(value: &str) -> Option<Self> {
        if value.eq_ignore_ascii_case("normal") { Some(Self::Normal) }
        else if value.eq_ignore_ascii_case("large") { Some(Self::Large) }
        else if value.eq_ignore_ascii_case("graph") { Some(Self::Graph) }
        else { None }
    }
    pub const fn name(self) -> &'static str {
        match self { Self::Normal => "normal", Self::Large => "large", Self::Graph => "graph" }
    }
}

pub const HISTORY_RANGES: [u16; 6] = [1, 5, 30, 60, 360, 1440];
pub const TREND_POINTS: usize = 128;
/// Reserved marker. All valid samples, including zero and negative power, differ from it.
pub const MISSING: i32 = i32::MIN;
const HISTORY_SLOTS: usize = 721;

pub fn range_label(minutes: u16) -> &'static str {
    match minutes { 1 => "1m", 30 => "30m", 60 => "1h", 360 => "6h", 1440 => "24h", _ => "5m" }
}

/// Fixed RAM, deciwatt averages: 5-second buckets for one hour and 2-minute buckets for one day.
/// Uptime timestamps keep history independent of NTP adjustments and the millisecond counter wrap.
pub struct PowerHistory { short: HistoryRing, long: HistoryRing }

struct HistoryRing {
    values: [i32; HISTORY_SLOTS],
    bucket: Option<u64>,
    sum: i64,
    count: u32,
}

impl HistoryRing {
    const fn new() -> Self {
        Self { values: [MISSING; HISTORY_SLOTS], bucket: None, sum: 0, count: 0 }
    }
    fn advance(&mut self, bucket: u64) {
        if let Some(old) = self.bucket {
            if bucket <= old { return; }
            if bucket - old >= HISTORY_SLOTS as u64 {
                self.values.fill(MISSING);
            } else {
                for b in old + 1..=bucket { self.values[b as usize % HISTORY_SLOTS] = MISSING; }
            }
        }
        self.bucket = Some(bucket);
        self.sum = 0;
        self.count = 0;
    }
    fn record(&mut self, bucket: u64, value: Option<i32>) {
        if self.bucket.is_some_and(|old| bucket < old) { return; }
        self.advance(bucket);
        if let Some(value) = value.filter(|_| self.count < u32::MAX) {
            self.sum += i64::from(value);
            self.count += 1;
            self.values[bucket as usize % HISTORY_SLOTS] = (self.sum / i64::from(self.count)) as i32;
        }
    }
    fn get(&self, bucket: i64) -> i32 {
        let Some(latest) = self.bucket else { return MISSING; };
        if bucket < 0 || bucket as u64 > latest || latest - bucket as u64 >= HISTORY_SLOTS as u64 {
            MISSING
        } else { self.values[bucket as usize % HISTORY_SLOTS] }
    }
}

#[derive(Debug)]
pub struct PowerTrend {
    pub points: [i32; TREND_POINTS],
    /// A partially missing column can show an average dot, but never connect a line across the gap.
    pub complete: [bool; TREND_POINTS],
    /// Vertical bounds in 0.1 W units; always include zero and have a nonzero span.
    pub min: i32,
    pub max: i32,
    pub minutes: u16,
    pub has_data: bool,
}

impl PowerTrend {
    pub const fn new() -> Self {
        Self { points: [MISSING; TREND_POINTS], complete: [false; TREND_POINTS], min: 0, max: 10, minutes: 5, has_data: false }
    }
}

impl PowerHistory {
    pub const fn new() -> Self { Self { short: HistoryRing::new(), long: HistoryRing::new() } }
    pub fn record(&mut self, now_secs: u64, milliwatts: Option<i64>) {
        let value = milliwatts.map(|v| (v / 100).clamp(i64::from(i32::MIN) + 1, i64::from(i32::MAX)) as i32);
        self.short.record(now_secs / 5, value);
        self.long.record(now_secs / 120, value);
    }
    /// Rebuild only on new readings, settings changes, or a five-second timer, never every frame.
    pub fn plot(&mut self, now_secs: u64, minutes: u16, out: &mut PowerTrend) {
        let minutes = if HISTORY_RANGES.contains(&minutes) { minutes } else { 5 };
        let (ring, step) = if minutes <= 60 { (&mut self.short, 5i64) } else { (&mut self.long, 120i64) };
        ring.advance(now_secs / step as u64);
        out.minutes = minutes;
        out.min = 0;
        out.max = 0;
        out.has_data = false;
        let span = i64::from(minutes) * 60;
        let end = now_secs as i64 + 1;
        for (i, point) in out.points.iter_mut().enumerate() {
            let a = end - span + i as i64 * span / TREND_POINTS as i64;
            let b = end - span + (i + 1) as i64 * span / TREND_POINTS as i64 - 1;
            let first = a.div_euclid(step);
            let last = b.max(a).div_euclid(step);
            let mut sum = 0i64;
            let mut count = 0i64;
            let mut missing = false;
            for bucket in first..=last {
                let v = ring.get(bucket);
                if v == MISSING { missing = true; continue; }
                sum += i64::from(v);
                count += 1;
            }
            out.complete[i] = !missing && count > 0;
            *point = if count == 0 { MISSING } else { (sum / count) as i32 };
            if *point != MISSING {
                out.has_data = true;
                out.min = out.min.min(*point);
                out.max = out.max.max(*point);
            }
        }
        if out.min == out.max { out.max = out.min.saturating_add(10); }
    }
}

impl Default for PowerHistory { fn default() -> Self { Self::new() } }
impl Default for PowerTrend { fn default() -> Self { Self::new() } }

/// Private controller identity. Intentionally does not implement Debug.
#[derive(Clone, Copy)]
pub struct PowerConfig {
    pub seed: [u8; 32],
    pub device_node: u64,
}

impl PowerConfig {
    pub fn parse(bytes: &[u8]) -> Result<Self, &'static str> {
        let text = core::str::from_utf8(bytes).map_err(|_| "MATTER.TXT: not UTF-8")?;
        let mut seed = None;
        let mut device_node = 0x110;
        for line in text.trim_start_matches('\u{feff}').lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once('=').ok_or("MATTER.TXT: expected key=value")?;
            match key.trim() {
                "controller_seed" => {
                    if seed.is_some() {
                        return Err("MATTER.TXT: duplicate seed");
                    }
                    let value = value.trim().as_bytes();
                    if value.len() != 64 {
                        return Err("MATTER.TXT: seed needs 64 hex digits");
                    }
                    let mut parsed = [0; 32];
                    for (out, pair) in parsed.iter_mut().zip(value.chunks_exact(2)) {
                        let digit = |b: u8| -> Result<u8, &'static str> {
                            match b {
                                b'0'..=b'9' => Ok(b - b'0'),
                                b'a'..=b'f' => Ok(b - b'a' + 10),
                                b'A'..=b'F' => Ok(b - b'A' + 10),
                                _ => Err("MATTER.TXT: seed is not hex"),
                            }
                        };
                        *out = digit(pair[0])? * 16 + digit(pair[1])?;
                    }
                    if parsed == [0; 32] {
                        return Err("MATTER.TXT: empty identity");
                    }
                    seed = Some(parsed);
                }
                "device_node" => {
                    let value = value.trim();
                    device_node = if let Some(hex) = value.strip_prefix("0x") { u64::from_str_radix(hex, 16) } else { value.parse() }
                        .map_err(|_| "MATTER.TXT: bad node id")?;
                    if device_node == 0 || device_node > 0xffff_ffef_ffff_ffff {
                        return Err("MATTER.TXT: bad node id");
                    }
                }
                _ => return Err("MATTER.TXT: unknown key"),
            }
        }
        Ok(Self { seed: seed.ok_or("MATTER.TXT: seed missing")?, device_node })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerStatus {
    Disabled,
    Waiting,
    Fresh,
    Stale,
    Unavailable,
    Unsupported,
    ConfigError,
}

#[derive(Clone, Copy, Debug)]
pub struct PowerView {
    pub milliwatts: Option<i64>,
    pub status: PowerStatus,
    pub age_secs: u32,
}

#[derive(Clone, Copy)]
pub struct PowerState {
    pub status: PowerStatus,
    sample: Option<(i64, u32)>,
}

impl PowerState {
    pub const fn new(status: PowerStatus) -> Self {
        Self { status, sample: None }
    }
    pub fn received(&mut self, value: Option<i64>, now_ms: u32) {
        self.sample = value.map(|v| (v, now_ms));
        self.status = if value.is_some() { PowerStatus::Fresh } else { PowerStatus::Unavailable };
    }
    pub fn failed(&mut self) {
        self.status = if self.sample.is_some() { PowerStatus::Stale } else { PowerStatus::Waiting };
    }
    pub fn view(&self, now_ms: u32) -> PowerView {
        let age = self.sample.map(|(_, at)| now_ms.wrapping_sub(at)).unwrap_or(0);
        let status = if self.status == PowerStatus::Fresh && age > STALE_MS { PowerStatus::Stale } else { self.status };
        PowerView { milliwatts: self.sample.map(|(v, _)| v), status, age_secs: age / 1000 }
    }
}

/// One decimal place, rounded to nearest 0.1 W, including negative and i64::MIN.
pub fn watts(value: i64) -> String<32> {
    let mut text = String::new();
    let tenths = (value.unsigned_abs() + 50) / 100;
    let sign = if value < 0 && tenths != 0 { "-" } else { "" };
    let _ = write!(text, "{sign}{}.{:01} W", tenths / 10, tenths % 10);
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_averages_zero_signed_samples_and_never_invents_missing_power() {
        let mut h = PowerHistory::new();
        h.record(0, Some(0));
        h.record(1, Some(1000));
        assert_eq!(h.short.get(0), 5);
        h.record(5, None);
        assert_eq!(h.short.get(1), MISSING);
        h.record(10, Some(-500));
        assert_eq!(h.short.get(2), -5);
        h.record(15, Some(i64::MIN));
        assert_ne!(h.short.get(3), MISSING);
        h.record(20, Some(i64::MAX));
        assert_eq!(h.short.get(4), i32::MAX);
        assert!(core::mem::size_of::<PowerHistory>() < 6000);
    }
    #[test]
    fn history_retains_one_day_and_expires_short_samples_without_timestamp_wrap() {
        let mut h = PowerHistory::new();
        let start = u64::from(u32::MAX) / 1000 - 10;
        for t in (start..=start + 86400).step_by(5) { h.record(t, Some(343300)); }
        assert_eq!(h.short.get((start / 5) as i64), MISSING);
        let mut trend = PowerTrend::new();
        for minutes in HISTORY_RANGES {
            h.plot(start + 86400, minutes, &mut trend);
            assert!(trend.has_data);
            assert_eq!(trend.minutes, minutes);
            assert!(trend.points.iter().filter(|&&v| v != MISSING).all(|&v| v == 3433));
        }
        h.plot(start + 2 * 86400 + 120, 1440, &mut trend);
        assert!(!trend.has_data);
    }
    #[test]
    fn plot_keeps_missing_gaps_and_shows_partial_startup_in_a_24_hour_window() {
        let mut h = PowerHistory::new();
        let mut trend = PowerTrend::new();
        h.record(0, Some(343300));
        h.plot(0, 1440, &mut trend);
        assert!(trend.has_data);
        assert_eq!(*trend.points.last().unwrap(), 3433);
        assert!(!*trend.complete.last().unwrap());
        for t in (0..=300).step_by(5) {
            h.record(t, if (100..200).contains(&t) { None } else { Some(0) });
        }
        h.plot(300, 5, &mut trend);
        assert!(trend.has_data);
        assert!(trend.points[45..80].iter().all(|&v| v == MISSING));
        assert!(trend.points.iter().any(|&v| v == 0));
        assert!(trend.max > trend.min);
    }
    #[test]
    fn null_and_failed_reads_never_become_zero_or_fresh() {
        let mut state = PowerState::new(PowerStatus::Waiting);
        state.received(Some(343336), 1000);
        assert_eq!(state.view(16_000).status, PowerStatus::Fresh);
        assert_eq!(state.view(16_001).status, PowerStatus::Stale);
        state.failed();
        assert_eq!(state.view(1100).status, PowerStatus::Stale);
        assert_eq!(state.view(1100).milliwatts, Some(343336));
        state.received(None, 1200);
        assert_eq!(state.view(1200).milliwatts, None);
        assert_eq!(state.view(1200).status, PowerStatus::Unavailable);
        state.received(Some(0), u32::MAX - 1000);
        assert_eq!(state.view(1000).age_secs, 2);
        assert_eq!(watts(0).as_str(), "0.0 W");
    }
    #[test]
    fn signed_milliwatts_round_without_overflow() {
        assert_eq!(watts(343336).as_str(), "343.3 W");
        assert_eq!(watts(50).as_str(), "0.1 W");
        assert_eq!(watts(-50).as_str(), "-0.1 W");
        assert_eq!(watts(-49).as_str(), "0.0 W");
        assert_eq!(watts(i64::MIN).as_str(), "-9223372036854775.8 W");
    }
    #[test]
    fn identity_parser_rejects_partial_or_corrupted_secrets() {
        assert!(PowerConfig::parse(b"controller_seed=1234").is_err());
        assert!(PowerConfig::parse(b"").is_err());
        let config =
            PowerConfig::parse(b"controller_seed=0102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F20\ndevice_node=0x110")
                .unwrap();
        assert_eq!(config.seed[31], 32);
        assert_eq!(config.device_node, 0x110);
        assert!(PowerConfig::parse(b"controller_seed=0000000000000000000000000000000000000000000000000000000000000000").is_err());
    }
}
