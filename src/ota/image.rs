//! OTA で受け取ったイメージの先頭セクタ (IMAGE_DEF) の検査 (1.0.1〜)
//!
//! 1.0.0 までは manifest の SHA-256 が一致すれば他方区画に確定していた。Release には全 bin の `.bin` と
//! `.sha256` が付くので、manifest の `bin` と `sha256` を書き換えるだけで (悪意あるコードを用意しなくても)
//! OTA 機能を持たない非 TBYB のイメージ (`ticker-plain.bin`、`layer*.bin` など) を入れられ、buy 無しで
//! 居座る = OTA 到達保証 (docs/ticker.md §8) が効かなくなる (review/fw-security-audit-2026-10-28.md H1)。
//!
//! ここでは `commit_first_sector` の前に RAM 上の先頭セクタを解釈し、次を確かめる:
//!
//! 1. bootrom が走査する先頭 4 kB (`SCAN_LEN`) にブロック (開始 / 終了マーカー、項目の長さの整合) がある
//! 2. IMAGE_TYPE 項目が EXE / Secure / Arm / RP2350 で、**TBYB フラグが立っている**
//! 3. VERSION 項目があり、manifest の `version` と一致する (`src/image_def.rs` の畳み込み: minor × 100 + patch)
//!
//! ブロック形式は RP2350 データシート §5.9 (embassy-rp 0.9 `block.rs` と同じ値)。このモジュールは
//! 本クレートの他の部分に依存しない (ホストの `tools/ticker-tests` で検査する)。

/// bootrom が IMAGE_DEF を探す範囲 (先頭 4 kB、§5.9.1)
pub const SCAN_LEN: usize = 4096;

/// `PICOBIN_BLOCK_MARKER_START`
const MARKER_START: u32 = 0xffff_ded3;
/// `PICOBIN_BLOCK_MARKER_END`
const MARKER_END: u32 = 0xab12_3579;
/// 項目の種類 (下位 7 bit)。bit 7 が立っていれば長さが 2 バイト (2BS)
const ITEM_KIND_MASK: u8 = 0x7f;
const ITEM_2BS_FLAG: u8 = 0x80;
const ITEM_IMAGE_TYPE: u8 = 0x42;
const ITEM_VERSION: u8 = 0x48;
const ITEM_LAST: u8 = 0xff;
/// 1 ブロックの項目数の上限 (ブロックは 4 kB に収まるので 1024 ワードを超えない)
const MAX_ITEM_WORDS: usize = SCAN_LEN / 4;

/// IMAGE_TYPE のビット (embassy-rp `block.rs` / データシート §5.9.3.1)
const TYPE_EXE: u16 = 0x0001;
const TYPE_KIND_MASK: u16 = 0x0003;
const TYPE_SECURITY_MASK: u16 = 0x0030;
const TYPE_SECURITY_S: u16 = 0x0020;
const TYPE_CPU_MASK: u16 = 0x0300;
const TYPE_CPU_ARM: u16 = 0x0000;
const TYPE_CHIP_MASK: u16 = 0x7000;
const TYPE_CHIP_RP2350: u16 = 0x1000;
const TYPE_TBYB: u16 = 0x8000;

/// 先頭セクタから読み取った IMAGE_DEF の要点
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageDef {
    /// IMAGE_TYPE 項目の値
    pub image_type: u16,
    /// VERSION 項目の (major, minor)。無ければ None
    pub version: Option<(u16, u16)>,
}

impl ImageDef {
    /// TBYB フラグが立っている (FLASH_UPDATE 起動でだけ選ばれ、`explicit_buy` まで確定しない)
    pub fn is_tbyb(&self) -> bool {
        self.image_type & TYPE_TBYB != 0
    }

    /// EXE / Secure / Arm / RP2350 (本クレートの `IMAGE_TYPE_FLAGS` と同じ組み合わせ)
    pub fn is_rp2350_arm_exe(&self) -> bool {
        self.image_type & TYPE_KIND_MASK == TYPE_EXE
            && self.image_type & TYPE_SECURITY_MASK == TYPE_SECURITY_S
            && self.image_type & TYPE_CPU_MASK == TYPE_CPU_ARM
            && self.image_type & TYPE_CHIP_MASK == TYPE_CHIP_RP2350
    }
}

/// 検査の失敗理由 (`OtaError` に写して LCD に出す)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// 先頭 4 kB に整合したブロックが無い、または IMAGE_TYPE 項目が無い
    NoImageDef,
    /// EXE / Secure / Arm / RP2350 でない
    NotExecutable,
    /// TBYB フラグが無い (buy 無しで確定してしまうイメージ)
    NotTbyb,
    /// VERSION 項目が無い (bootrom が新旧を判断できず、manifest との照合もできない)
    NoVersion,
    /// VERSION 項目が manifest の版数と違う
    VersionMismatch,
}

impl ImageError {
    /// LCD 用の短い名前
    pub fn label(self) -> &'static str {
        match self {
            ImageError::NoImageDef => "no IMAGE_DEF",
            ImageError::NotExecutable => "not RP2350 exe",
            ImageError::NotTbyb => "image not TBYB",
            ImageError::NoVersion => "image has no version",
            ImageError::VersionMismatch => "image version mismatch",
        }
    }
}

/// manifest の semver を IMAGE_DEF の (major, minor) に畳む (`src/image_def.rs` と同じ規則)。
/// patch ≥ 100 や minor × 100 + patch が u16 を超える版数は IMAGE_DEF に載らないので None。
pub fn expected_version(major: u16, minor: u16, patch: u16) -> Option<(u16, u16)> {
    if patch >= 100 {
        return None;
    }
    let folded = minor.checked_mul(100)?.checked_add(patch)?;
    Some((major, folded))
}

/// 先頭セクタ (`SCAN_LEN` 以上あれば先頭 4 kB だけを見る) から IMAGE_DEF を読む。
///
/// bootrom と同じく **最初に整合したブロック** を採用する (それより前の開始マーカーは、ブロックとして
/// 整合しなければ飛ばす)。採用したブロックは次を満たさなければ IMAGE_DEF と認めない (`NoImageDef`):
/// 第 1 項目が IMAGE_TYPE、IMAGE_TYPE / VERSION が 1 回ずつ、リンクが 0 (自分だけのブロックループ)。
/// 本クレートの `firmware_image_def!()` は常にこの形なので、bootrom が別のブロックを選ぶ余地が無い
/// (複数ブロックや別のループを持つ細工イメージは、SHA-256 が合っていても入れない)。
pub fn parse(first_sector: &[u8]) -> Result<ImageDef, ImageError> {
    let scan = &first_sector[..first_sector.len().min(SCAN_LEN)];
    (0..scan.len() / 4)
        .map(|i| i * 4)
        .filter(|&at| word_at(scan, at) == Some(MARKER_START))
        .find_map(|at| parse_block_at(scan, at))
        .ok_or(ImageError::NoImageDef)?
        .ok_or(ImageError::NoImageDef)
}

/// 先頭セクタを解釈し、TBYB の RP2350 実行イメージで、版数が manifest の `major.minor.patch` と一致することを確かめる
pub fn check(first_sector: &[u8], major: u16, minor: u16, patch: u16) -> Result<ImageDef, ImageError> {
    let def = parse(first_sector)?;
    if !def.is_rp2350_arm_exe() {
        return Err(ImageError::NotExecutable);
    }
    if !def.is_tbyb() {
        return Err(ImageError::NotTbyb);
    }
    let version = def.version.ok_or(ImageError::NoVersion)?;
    if Some(version) != expected_version(major, minor, patch) {
        return Err(ImageError::VersionMismatch);
    }
    Ok(def)
}

fn word_at(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// `at` にある開始マーカーから項目を辿る。
///
/// - 外側の `None`: ブロックとして整合しない (項目の長さが範囲外、LAST 項目の長さが実際と違う、終了マーカーが無い)。
///   bootrom もこれをブロックと見なさないので、次の開始マーカーを試す。
/// - `Some(None)`: 整合したブロックだが IMAGE_DEF として認めない (第 1 項目が IMAGE_TYPE でない、IMAGE_TYPE /
///   VERSION の重複、リンクが 0 でない)。bootrom はこのブロックを採用するので、後ろのブロックは見ない。
fn parse_block_at(scan: &[u8], at: usize) -> Option<Option<ImageDef>> {
    let mut pos = at + 4;
    let mut words = 0usize;
    let mut image_type: Option<u16> = None;
    let mut version: Option<(u16, u16)> = None;
    let mut acceptable = true;
    loop {
        let header = word_at(scan, pos)?;
        let kind = (header & 0xff) as u8;
        if kind == ITEM_LAST {
            let total = ((header >> 8) & 0xffff) as usize;
            if total != words {
                return None;
            }
            // LAST 項目 (1 ワード) → リンク (1 ワード) → 終了マーカー
            let link = word_at(scan, pos + 4)?;
            if word_at(scan, pos + 8)? != MARKER_END {
                return None;
            }
            let def = image_type.map(|image_type| ImageDef { image_type, version });
            return Some(def.filter(|_| acceptable && link == 0));
        }
        let size = if kind & ITEM_2BS_FLAG != 0 {
            ((header >> 8) & 0xffff) as usize
        } else {
            ((header >> 8) & 0xff) as usize
        };
        if size == 0 || words + size > MAX_ITEM_WORDS || pos + size * 4 > scan.len() {
            return None;
        }
        let is_1bs = kind & ITEM_2BS_FLAG == 0;
        match kind & ITEM_KIND_MASK {
            k if k == ITEM_IMAGE_TYPE && is_1bs => {
                // IMAGE_DEF は IMAGE_TYPE で始まる (§5.9.3)。2 回目は不正
                if words != 0 || image_type.is_some() {
                    acceptable = false;
                }
                image_type = Some((header >> 16) as u16);
            }
            k if k == ITEM_VERSION && is_1bs && size >= 2 => {
                if version.is_some() {
                    acceptable = false;
                }
                let value = word_at(scan, pos + 4)?;
                version = Some(((value >> 16) as u16, (value & 0xffff) as u16));
            }
            _ => {}
        }
        pos += size * 4;
        words += size;
    }
}
