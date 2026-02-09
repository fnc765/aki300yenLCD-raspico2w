//! PIO プログラム定義
//!
//! LTA042B010F は NCLK の**立ち下がりエッジ**でデータをサンプルする。
//! したがって、PIO sideset で NCLK を制御し、
//! データ出力時に NCLK=LOW → LCD がサンプルという順序にする。

/// SM0: ピクセルデータ + NCLK 出力テスト用 PIO プログラム
///
/// - OUT ピン: GP2-GP19 (RGB 18bit)
/// - SIDESET ピン: GP20 (NCLK)
/// - autopull 有効
///
/// 動作:
/// 1. out pins, 18 + side 0 → RGB データ出力 + NCLK LOW (LCD サンプル)
/// 2. nop + side 1 → NCLK HIGH (次のデータ準備)
pub const PIXEL_OUT_PROGRAM: &str = r#"
.side_set 1

.wrap_target
    out pins, 18  side 0    ; RGB出力 + NCLK=LOW (LCDがサンプル)
    nop           side 1    ; NCLK=HIGH (データ安定)
.wrap
"#;

// NOTE: 実際の PIO プログラムは pio_proc::pio_asm! マクロで
// コンパイル時に生成する。上記は設計ドキュメント用。
//
// 使用例:
// ```rust
// use embassy_rp::pio::program::pio_asm;
//
// let prg = pio_asm!(
//     ".side_set 1",
//     ".wrap_target",
//     "    out pins, 18  side 0",
//     "    nop           side 1",
//     ".wrap",
// );
// ```
