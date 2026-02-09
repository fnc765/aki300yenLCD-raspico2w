//! ピン定義
//!
//! LTA042B010F (400x96 RGB666 TFT LCD) の接続ピン割り当て
//!
//! # ピン割り当て (案A: RGB666 フル接続)
//!
//! | GPIO | 用途 | LCD ピン |
//! |------|------|---------|
//! | GP0  | UART0 TX (デバッグ) | - |
//! | GP1  | UART0 RX (デバッグ) | - |
//! | GP2  | B5 (青 MSB) | Pin 8 |
//! | GP3  | B4 | Pin 9 |
//! | GP4  | B3 | Pin 10 |
//! | GP5  | B2 | Pin 11 |
//! | GP6  | B1 | Pin 12 |
//! | GP7  | B0 (青 LSB) | Pin 13 |
//! | GP8  | G5 (緑 MSB) | Pin 15 |
//! | GP9  | G4 | Pin 16 |
//! | GP10 | G3 | Pin 17 |
//! | GP11 | G2 | Pin 18 |
//! | GP12 | G1 | Pin 19 |
//! | GP13 | G0 (緑 LSB) | Pin 20 |
//! | GP14 | R5 (赤 MSB) | Pin 22 |
//! | GP15 | R4 | Pin 23 |
//! | GP16 | R3 | Pin 24 |
//! | GP17 | R2 | Pin 25 |
//! | GP18 | R1 | Pin 26 |
//! | GP19 | R0 (赤 LSB) | Pin 27 |
//! | GP20 | NCLK (ドットクロック) | Pin 2 |
//! | GP21 | HSYNC (水平同期) | Pin 4 |
//! | GP22 | VSYNC (垂直同期) | Pin 5 |

/// RGB データのベース GPIO (GP2)
pub const RGB_BASE_PIN: u8 = 2;

/// RGB データのピン数 (18本: B[5:0] + G[5:0] + R[5:0])
pub const RGB_PIN_COUNT: u8 = 18;

/// NCLK (ドットクロック) の GPIO (GP20)
pub const NCLK_PIN: u8 = 20;

/// HSYNC (水平同期) の GPIO (GP21)
pub const HSYNC_PIN: u8 = 21;

/// VSYNC (垂直同期) の GPIO (GP22)
pub const VSYNC_PIN: u8 = 22;
