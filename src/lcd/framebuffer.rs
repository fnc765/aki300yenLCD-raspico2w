//! フレームバッファ管理
//!
//! 512×96 パディング方式: 各ライン = [107 BLACK | 400 active | 5 BLACK]
//! DMA 転送時にブランキング挿入不要で、Layer 4 と同一の転送パターンを使用。

#![allow(dead_code)]

use crate::lcd::timing::{H_ACTIVE, H_BLANK_BEFORE_ACTIVE, H_TOTAL};
use embedded_graphics::geometry::Size;
use embedded_graphics::pixelcolor::{Rgb666, RgbColor};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;

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

/// アクティブ領域の最大 X 座標 (パターンマッチ用)
const MAX_X: u32 = H_ACTIVE - 1; // 399
/// アクティブ領域の最大 Y 座標 (パターンマッチ用)
const MAX_Y: u32 = ACTIVE_HEIGHT as u32 - 1; // 95

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

// ============================================================
// embedded-graphics DrawTarget 実装
// ============================================================

impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(H_ACTIVE as u32, ACTIVE_HEIGHT as u32) // 400x96
    }
}

impl DrawTarget for FrameBuffer {
    type Color = Rgb666;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            // Point の x, y は i32。範囲外は無視。
            if let Ok((x @ 0..=MAX_X, y @ 0..=MAX_Y)) = coord.try_into() {
                let x: u32 = x;
                let y: u32 = y;
                let word = rgb666(color.r() as u32, color.g() as u32, color.b() as u32);
                self.set_pixel(x as usize, y as usize, word);
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Self::Color) -> Result<(), Self::Error> {
        let word = rgb666(color.r() as u32, color.g() as u32, color.b() as u32);

        // クリッピング: FrameBuffer のアクティブ領域 (0..400, 0..96) と交差する部分のみ
        let display_area = Rectangle::new(
            embedded_graphics::geometry::Point::zero(),
            Size::new(H_ACTIVE as u32, ACTIVE_HEIGHT as u32),
        );

        let clipped = area.intersection(&display_area);
        if clipped.size.width > 0 && clipped.size.height > 0 {
            let x_start = clipped.top_left.x as usize;
            let y_start = clipped.top_left.y as usize;
            let x_end = x_start + clipped.size.width as usize;
            let y_end = y_start + clipped.size.height as usize;

            for y in y_start..y_end {
                let row_start = y * LINE_WIDTH + H_BLANK_BEFORE_ACTIVE as usize + x_start;
                let row_end = row_start + (x_end - x_start);
                self.data[row_start..row_end].fill(word);
            }
        }

        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        let word = rgb666(color.r() as u32, color.g() as u32, color.b() as u32);
        // パディング領域は変更しない（BLACKのまま）
        for y in 0..ACTIVE_HEIGHT {
            let start = y * LINE_WIDTH + H_BLANK_BEFORE_ACTIVE as usize;
            let end = start + H_ACTIVE as usize;
            self.data[start..end].fill(word);
        }
        Ok(())
    }
}
