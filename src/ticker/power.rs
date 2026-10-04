//! Matter power configuration and sample freshness, independent of hardware.
use core::fmt::Write as _;
use heapless::String;

pub const CONFIG_MAX: usize = 256;
pub const STALE_MS: u32 = 15_000;

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
