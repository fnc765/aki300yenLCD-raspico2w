//! フレームバッファ管理
//!
//! 512×96 パディング方式: 各ライン = [107 BLACK | 400 active | 5 BLACK]
//! DMA 転送時にブランキング挿入不要で、Layer 4 と同一の転送パターンを使用。

#![allow(dead_code)]

use crate::lcd::timing::{H_ACTIVE, H_BLANK_BEFORE_ACTIVE, H_TOTAL};

/// ライン幅 (パディング込み)
pub const LINE_WIDTH: usize = H_TOTAL as usize; // 512

/// アクティブ高さ
pub const ACTIVE_HEIGHT: usize = 96;

/// フレームバッファサイズ (ワード数)
pub const FB_SIZE: usize = LINE_WIDTH * ACTIVE_HEIGHT; // 49,152

/// 6ビット値のビット順を反転 (MSB↔LSB)
///
/// PIO OUT pins (right shift) のビットマッピングにより、
/// bit 0 → GP2 (B5=青MSB) となるため、チャンネル内ビット反転が必要。
pub const fn reverse6(v: u32) -> u32 {
    let v = v & 0x3F;
    ((v & 0x20) >> 5)
        | ((v & 0x10) >> 3)
        | ((v & 0x08) >> 1)
        | ((v & 0x04) << 1)
        | ((v & 0x02) << 3)
        | ((v & 0x01) << 5)
}

/// RGB666 ピクセルワード生成 (ビット順反転込み)
///
/// 各チャンネルは 0-63 の範囲。
/// OUT base=GP2, right shift で bit0→GP2。
pub const fn rgb666(r: u32, g: u32, b: u32) -> u32 {
    (reverse6(r) << 12) | (reverse6(g) << 6) | reverse6(b)
}

/// BLACK ピクセル
pub const BLACK: u32 = 0;

/// フレームバッファ
///
/// 内部レイアウト: `[u32; 512 * 96]`
/// 各ライン: `[107 BLACK] [400 active pixels] [5 BLACK]`
///
/// パディング領域は初期化時に BLACK で埋められ、
/// `set_pixel()` / `clear()` はアクティブ領域のみ操作する。
///
/// メモリ使用量: 49,152 × 4 = 196,608 bytes (192 KB) / バッファ
pub struct FrameBuffer {
    pub data: [u32; FB_SIZE],
}

impl FrameBuffer {
    /// 新しいフレームバッファを作成 (全ピクセル BLACK)
    ///
    /// パディング領域も含めて全てゼロ初期化。
    pub const fn new() -> Self {
        Self {
            data: [BLACK; FB_SIZE],
        }
    }

    /// アクティブ領域のピクセルを設定
    ///
    /// x: 0..400, y: 0..96
    /// パディング領域は変更しない。
    #[inline]
    pub fn set_pixel(&mut self, x: usize, y: usize, color: u32) {
        debug_assert!(x < H_ACTIVE as usize && y < ACTIVE_HEIGHT, "set_pixel: ({}, {}) out of range", x, y);
        if x < H_ACTIVE as usize && y < ACTIVE_HEIGHT {
            let offset = y * LINE_WIDTH + H_BLANK_BEFORE_ACTIVE as usize + x;
            self.data[offset] = color;
        }
    }

    /// アクティブ領域のピクセルを取得
    #[inline]
    pub fn get_pixel(&self, x: usize, y: usize) -> u32 {
        debug_assert!(x < H_ACTIVE as usize && y < ACTIVE_HEIGHT, "get_pixel: ({}, {}) out of range", x, y);
        if x < H_ACTIVE as usize && y < ACTIVE_HEIGHT {
            let offset = y * LINE_WIDTH + H_BLANK_BEFORE_ACTIVE as usize + x;
            self.data[offset]
        } else {
            BLACK
        }
    }

    /// DMA 転送用ライン参照を取得
    ///
    /// ライン y (0..96) の 512 ワード全体を返す。
    #[inline]
    pub fn row_slice(&self, y: usize) -> &[u32] {
        debug_assert!(y < ACTIVE_HEIGHT, "row_slice: y={} out of range (max {})", y, ACTIVE_HEIGHT - 1);
        let start = y * LINE_WIDTH;
        &self.data[start..start + LINE_WIDTH]
    }

    /// アクティブ領域を指定色でクリア
    ///
    /// パディング領域 (BLACK) は変更しない。
    pub fn clear(&mut self, color: u32) {
        for y in 0..ACTIVE_HEIGHT {
            let start = y * LINE_WIDTH + H_BLANK_BEFORE_ACTIVE as usize;
            let end = start + H_ACTIVE as usize;
            self.data[start..end].fill(color);
        }
    }
}
