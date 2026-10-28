//! OTA で受け取ったイメージの先頭セクタ (IMAGE_DEF) の検査 (`src/ota/image.rs`)。
//!
//! 1.0.1〜: SHA-256 が manifest と一致しても、IMAGE_DEF が無い / TBYB で無い / 版数が manifest と違う
//! イメージは他方区画に確定しない (非 TBYB の `*-plain.bin` や `layer*.bin` を manifest で指されても入れない)。

use crate::ota_image::{self, ImageDef, ImageError, SCAN_LEN};

const MARKER_START: u32 = 0xffff_ded3;
const MARKER_END: u32 = 0xab12_3579;
const ITEM_IMAGE_TYPE: u8 = 0x42;
const ITEM_VERSION: u8 = 0x48;
const ITEM_LAST: u8 = 0xff;

/// EXE | Secure | Arm (0x0000) | RP2350
const TYPE_RP2350_ARM_S: u16 = 0x0001 | 0x0020 | 0x1000;
const TYPE_TBYB: u16 = 0x8000;

fn item_1bs(value: u16, words: u8, kind: u8) -> u32 {
    ((value as u32) << 16) | ((words as u32) << 8) | kind as u32
}

fn item_2bs(words: u16, kind: u8) -> u32 {
    ((words as u32) << 8) | kind as u32
}

/// ブロック 1 個分のワード列 (開始マーカー、項目、LAST、リンク、終了マーカー)
fn block(items: &[u32], link: u32) -> Vec<u8> {
    let mut words: Vec<u32> = vec![MARKER_START];
    words.extend_from_slice(items);
    words.push(item_2bs(items.len() as u16, ITEM_LAST));
    words.push(link);
    words.push(MARKER_END);
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

/// `src/image_def.rs` の `firmware_image_def()` と同じ形のブロックを `prefix` の後ろに置いた先頭セクタ
fn sector(prefix: usize, items: &[u32]) -> Vec<u8> {
    let mut out = vec![0x11u8; prefix];
    out.extend_from_slice(&block(items, 0));
    out.resize(4096, 0xff);
    out
}

fn tbyb_items(major: u16, minor: u16) -> [u32; 3] {
    [
        item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE),
        item_1bs(0, 2, ITEM_VERSION),
        ((major as u32) << 16) | minor as u32,
    ]
}

// ============================================================
// 解釈
// ============================================================

#[test]
fn parses_tbyb_image_def_after_vector_table() {
    let s = sector(0xc0, &tbyb_items(1, 0));
    let def = ota_image::parse(&s).unwrap();
    assert_eq!(def.image_type, TYPE_RP2350_ARM_S | TYPE_TBYB);
    assert_eq!(def.version, Some((1, 0)));
    assert!(def.is_tbyb());
    assert!(def.is_rp2350_arm_exe());
}

#[test]
fn parses_image_def_without_version_item() {
    let s = sector(0xc0, &[item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE)]);
    let def = ota_image::parse(&s).unwrap();
    assert_eq!(def.version, None);
}

#[test]
fn skips_unknown_items_and_reads_version_after_them() {
    let items = [
        item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE),
        item_1bs(0, 2, 0x44), // ENTRY_POINT (2 ワード: 不明な項目として飛ばす)
        0x2000_0000,
        item_1bs(0, 2, ITEM_VERSION),
        601, // major 0, minor 601
    ];
    let def = ota_image::parse(&sector(0x100, &items)).unwrap();
    assert_eq!(def.version, Some((0, 601)));
}

#[test]
fn ignores_marker_that_is_not_a_consistent_block_and_uses_the_next_one() {
    // 先頭の開始マーカーの後ろが壊れていれば bootrom もブロックと見なさないので、次を採用する
    let mut s = vec![0x11u8; 0x80];
    s.extend_from_slice(&MARKER_START.to_le_bytes());
    s.extend_from_slice(&[0u8; 12]); // 項目サイズ 0 → 不整合
    s.resize(0xc0, 0x11);
    s.extend_from_slice(&block(&tbyb_items(1, 0), 0));
    s.resize(4096, 0xff);
    assert_eq!(ota_image::parse(&s).unwrap().version, Some((1, 0)));
}

#[test]
fn first_consistent_block_wins_even_if_a_later_block_is_tbyb() {
    // 非 TBYB のブロックの後ろに TBYB のブロックを置く細工: bootrom は最初のブロックを採用するので、こちらも
    let mut s = vec![0x11u8; 0xc0];
    s.extend_from_slice(&block(&[item_1bs(TYPE_RP2350_ARM_S, 1, ITEM_IMAGE_TYPE), item_1bs(0, 2, ITEM_VERSION), 1 << 16], 0));
    s.extend_from_slice(&block(&tbyb_items(1, 0), 0));
    s.resize(4096, 0xff);
    assert_eq!(ota_image::check(&s, 1, 0, 0), Err(ImageError::NotTbyb));
}

#[test]
fn rejects_block_whose_link_is_not_a_self_loop() {
    // リンクが別のブロックを指す = ブロックループに他のブロックがある。bootrom がどれを使うか保証できないので入れない
    let mut s = vec![0x11u8; 0xc0];
    s.extend_from_slice(&block(&tbyb_items(1, 0), 4 * 6));
    s.extend_from_slice(&block(&[item_1bs(TYPE_RP2350_ARM_S, 1, ITEM_IMAGE_TYPE)], (-(4i32 * 6)) as u32));
    s.resize(4096, 0xff);
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_block_whose_first_item_is_not_image_type() {
    // IMAGE_DEF は IMAGE_TYPE で始まる (§5.9.3)。後ろに IMAGE_TYPE があっても認めない
    let items = [
        item_1bs(0, 2, ITEM_VERSION),
        1 << 16,
        item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE),
    ];
    assert_eq!(ota_image::parse(&sector(0xc0, &items)), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_duplicate_image_type_or_version_items() {
    // [非 TBYB, TBYB] の順に IMAGE_TYPE を 2 つ置き、bootrom が先勝ちで読むと非 TBYB になる細工
    let twice_type = [
        item_1bs(TYPE_RP2350_ARM_S, 1, ITEM_IMAGE_TYPE),
        item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE),
        item_1bs(0, 2, ITEM_VERSION),
        1 << 16,
    ];
    assert_eq!(ota_image::parse(&sector(0xc0, &twice_type)), Err(ImageError::NoImageDef));
    let twice_version = [
        item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE),
        item_1bs(0, 2, ITEM_VERSION),
        1 << 16,
        item_1bs(0, 2, ITEM_VERSION),
        2 << 16,
    ];
    assert_eq!(ota_image::parse(&sector(0xc0, &twice_version)), Err(ImageError::NoImageDef));
}

#[test]
fn two_byte_size_image_type_and_short_version_items_are_not_recognised() {
    // 2BS 形式 (0xc2) の IMAGE_TYPE は IMAGE_TYPE ではない → 第 1 項目が IMAGE_TYPE でないので IMAGE_DEF でない
    let s = sector(0xc0, &[item_2bs(1, ITEM_IMAGE_TYPE | 0x80)]);
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
    // サイズ 1 の VERSION (値のワードが無い) は VERSION と見なさない
    let s = sector(0xc0, &[item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE), item_1bs(0, 1, ITEM_VERSION)]);
    assert_eq!(ota_image::check(&s, 1, 0, 0), Err(ImageError::NoVersion));
}

#[test]
fn block_ending_exactly_at_the_scan_limit_is_accepted() {
    let s = sector(SCAN_LEN - 4 * 7, &tbyb_items(1, 0));
    assert_eq!(ota_image::parse(&s).unwrap().version, Some((1, 0)));
    // 1 ワードでもはみ出せば認めない
    let s = sector(SCAN_LEN - 4 * 6, &tbyb_items(1, 0));
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_sector_without_start_marker() {
    let s = vec![0xffu8; 4096];
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_block_with_wrong_end_marker() {
    let mut s = sector(0xc0, &tbyb_items(1, 0));
    let end = 0xc0 + 4 * (1 + 3 + 1 + 1);
    s[end..end + 4].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_block_whose_last_item_size_disagrees() {
    let mut s = sector(0xc0, &tbyb_items(1, 0));
    let last = 0xc0 + 4 * (1 + 3);
    s[last..last + 4].copy_from_slice(&item_2bs(2, ITEM_LAST).to_le_bytes());
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_block_without_image_type_item() {
    let s = sector(0xc0, &[item_1bs(0, 2, ITEM_VERSION), 1 << 16]);
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn rejects_block_that_runs_past_the_sector() {
    // 項目の size が巨大でも範囲外を読まない
    let mut s = sector(4096 - 4 * 6, &tbyb_items(1, 0));
    let huge = 4096 - 4 * 6 + 4;
    s[huge..huge + 4].copy_from_slice(&item_1bs(0, 200, ITEM_IMAGE_TYPE).to_le_bytes());
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn ignores_marker_beyond_the_first_4kb() {
    let mut s = sector(0xc0, &tbyb_items(1, 0));
    s.truncate(0xc0); // ブロックを消す
    s.resize(SCAN_LEN, 0xff);
    let mut tail = sector(0, &tbyb_items(1, 0));
    s.append(&mut tail); // 4 kB より後ろにだけブロックがある
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

#[test]
fn short_sector_is_not_an_image() {
    assert_eq!(ota_image::parse(&[0u8; 16]), Err(ImageError::NoImageDef));
    assert_eq!(ota_image::parse(&[]), Err(ImageError::NoImageDef));
}

#[test]
fn marker_must_be_word_aligned() {
    let mut s = vec![0x11u8; 0xc1];
    s.extend_from_slice(&sector(0, &tbyb_items(1, 0))[..4 * 6]);
    s.resize(4096, 0xff);
    assert_eq!(ota_image::parse(&s), Err(ImageError::NoImageDef));
}

// ============================================================
// 検査 (manifest の版数と突き合わせる)
// ============================================================

#[test]
fn check_accepts_tbyb_image_whose_version_matches_manifest() {
    let s = sector(0xc0, &tbyb_items(1, 0));
    let def = ota_image::check(&s, 1, 0, 0).unwrap();
    assert_eq!(def.version, Some((1, 0)));
    let s = sector(0xc0, &tbyb_items(0, 601));
    assert!(ota_image::check(&s, 0, 6, 1).is_ok());
}

#[test]
fn check_rejects_image_without_tbyb_flag() {
    // Release の *-plain.bin / layer*.bin: SHA-256 は公開されているので manifest で指せるが、入れない
    let s = sector(0xc0, &[item_1bs(TYPE_RP2350_ARM_S, 1, ITEM_IMAGE_TYPE), item_1bs(0, 2, ITEM_VERSION), 1 << 16]);
    assert_eq!(ota_image::check(&s, 1, 0, 0), Err(ImageError::NotTbyb));
}

#[test]
fn check_rejects_version_mismatch() {
    let s = sector(0xc0, &tbyb_items(0, 601));
    assert_eq!(ota_image::check(&s, 1, 0, 0), Err(ImageError::VersionMismatch));
    assert_eq!(ota_image::check(&s, 0, 6, 2), Err(ImageError::VersionMismatch));
}

#[test]
fn check_rejects_image_without_version_item() {
    let s = sector(0xc0, &[item_1bs(TYPE_RP2350_ARM_S | TYPE_TBYB, 1, ITEM_IMAGE_TYPE)]);
    assert_eq!(ota_image::check(&s, 1, 0, 0), Err(ImageError::NoVersion));
}

#[test]
fn check_rejects_manifest_patch_that_image_def_cannot_carry() {
    // IMAGE_DEF の minor = minor × 100 + patch なので patch ≥ 100 は表せない (SKILL.md §5)
    let s = sector(0xc0, &tbyb_items(1, 100));
    assert_eq!(ota_image::check(&s, 1, 0, 100), Err(ImageError::VersionMismatch));
}

#[test]
fn check_rejects_non_rp2350_arm_secure_exe() {
    for bad in [
        0x0002 | 0x1000 | TYPE_TBYB,                   // DATA
        0x0001 | 0x0020 | TYPE_TBYB,                   // RP2040
        0x0001 | 0x0020 | 0x1000 | 0x0100 | TYPE_TBYB, // RISC-V
        0x0001 | 0x0010 | 0x1000 | TYPE_TBYB,          // Non-secure
    ] {
        let s = sector(0xc0, &[item_1bs(bad, 1, ITEM_IMAGE_TYPE), item_1bs(0, 2, ITEM_VERSION), 1 << 16]);
        assert_eq!(ota_image::check(&s, 1, 0, 0), Err(ImageError::NotExecutable), "type {bad:#06x}");
    }
}

#[test]
fn check_reports_missing_image_def_first() {
    assert_eq!(ota_image::check(&[0xffu8; 4096], 1, 0, 0), Err(ImageError::NoImageDef));
}

#[test]
fn expected_version_folds_patch_into_minor() {
    assert_eq!(ota_image::expected_version(0, 6, 1), Some((0, 601)));
    assert_eq!(ota_image::expected_version(1, 0, 0), Some((1, 0)));
    assert_eq!(ota_image::expected_version(0, 1, 99), Some((0, 199)));
    assert_eq!(ota_image::expected_version(0, 1, 100), None);
    assert_eq!(ota_image::expected_version(0, 656, 0), None); // minor × 100 が u16 を超える
}

#[test]
fn image_def_labels_are_short_for_the_lcd() {
    for e in [
        ImageError::NoImageDef,
        ImageError::NotExecutable,
        ImageError::NotTbyb,
        ImageError::NoVersion,
        ImageError::VersionMismatch,
    ] {
        assert!(e.label().len() <= 24, "{:?}", e);
    }
    let _: ImageDef = ota_image::parse(&sector(0xc0, &tbyb_items(1, 0))).unwrap();
}
