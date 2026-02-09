//! LTA042B010F タイミングパラメータ
//!
//! コミュニティ解析 (naruken, xcrosgs2wy) に基づく確定値

/// LCD 水平解像度
pub const H_ACTIVE: u32 = 400;

/// LCD 垂直解像度
pub const V_ACTIVE: u32 = 96;

/// 水平バックポーチ (HSYNC → 表示開始)
pub const H_BACK_PORCH: u32 = 107;

/// 水平フロントポーチ (表示終了 → 次 HSYNC)
pub const H_FRONT_PORCH: u32 = 5;

/// 水平合計クロック数 (1ライン)
pub const H_TOTAL: u32 = H_ACTIVE + H_BACK_PORCH + H_FRONT_PORCH; // 512

/// 垂直バックポーチ (VSYNC → 表示開始)
pub const V_BACK_PORCH: u32 = 16;

/// 垂直フロントポーチ (表示終了 → 次 VSYNC)
pub const V_FRONT_PORCH: u32 = 0;

/// 垂直合計ライン数 (1フレーム)
pub const V_TOTAL: u32 = V_ACTIVE + V_BACK_PORCH + V_FRONT_PORCH; // 112

/// VSYNC サンプリングオフセット (HSYNC 開始から N クロック後)
pub const VSYNC_SAMPLE_OFFSET: u32 = 98;

/// 目標フレームレート (Hz)
pub const TARGET_FPS: u32 = 60;

/// 目標ピクセルクロック周波数 (Hz)
/// H_TOTAL × V_TOTAL × FPS = 512 × 112 × 60 = 3,440,640 Hz
pub const PIXEL_CLOCK_HZ: u32 = H_TOTAL * V_TOTAL * TARGET_FPS;

/// システムクロック (Hz)
pub const SYS_CLOCK_HZ: u32 = 150_000_000;

/// PIO クロック分周比 (整数部)
/// NCLK は 2 PIO サイクルで 1 ピクセル (HIGH + LOW)
/// 分周比 = SYS_CLOCK / (PIXEL_CLOCK × 2) = 150_000_000 / (3_440_640 × 2) ≈ 21.8
pub const PIO_CLK_DIV_INT: u16 = (SYS_CLOCK_HZ / (PIXEL_CLOCK_HZ * 2)) as u16; // 21

/// PIO クロック分周比 (小数部, 0-255)
/// 余り = 150_000_000 - 21 × 3_440_640 × 2 = 150_000_000 - 144_506_880 = 5_493_120
/// frac = 5_493_120 × 256 / (3_440_640 × 2) ≈ 204
pub const PIO_CLK_DIV_FRAC: u8 = 204;
