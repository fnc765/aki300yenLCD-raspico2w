//! PIO プログラム定義
//!
//! LTA042B010F は NCLK の**立ち下がりエッジ**でデータをサンプルする。
//! したがって、PIO sideset で NCLK を制御し、
//! データ出力時に NCLK=LOW → LCD がサンプルという順序にする。
//!
//! # ⚠️ NCLK 停止リスク
//!
//! PIO プログラムは `pull block` を使用しており、TX FIFO が空の場合
//! ステートマシンが停止する。停止中は sideset (NCLK) の値が固定され、
//! NCLK クロックが停止する。
//!
//! **LTA042B010F のデータシートでは NCLK の停止はパネル損傷の
//! リスクがあるとされている。**
//!
//! - Layer 2: CPU が 225 writes/frame を供給（150MHz CPU で十分余裕あり）
//! - Layer 4+: DMA を使用して FIFO 供給を自動化すべき

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

// ============================================================
// Layer 2: HSYNC/VSYNC タイミング生成 PIO プログラム
// ============================================================
//
// ## アーキテクチャ
//
// 単一 SM (PIO0 SM0) で NCLK + HSYNC + VSYNC を同時生成する。
//
// - sideset 1 pin: NCLK (GP20) — 全命令で交互に 0/1
// - set 2 pins: HSYNC (GP21, bit0) + VSYNC (GP22, bit1)
// - CPU が TX FIFO にカウント値を供給、PIO が pull block で読む
//
// ## SET ピン値エンコーディング
//
// | 値   | HSYNC | VSYNC | 用途                         |
// |------|-------|-------|------------------------------|
// | 0b00 | 0(↓)  | 0(↓)  | HSYNC+VSYNC パルス中          |
// | 0b01 | 1(↑)  | 0(↓)  | VSYNC アクティブ中の非 HSYNC   |
// | 0b10 | 0(↓)  | 1(↑)  | 通常の HSYNC パルス            |
// | 0b11 | 1(↑)  | 1(↑)  | 通常の非アクティブ              |
//
// ## FIFO プロトコル (1 フレームあたり)
//
// CPU → TX FIFO 書き込み順序:
//
// ```text
// 1. push(PIO_HSYNC_COUNT)   // VSYNC ライン HSYNC パルスカウント
// 2. push(PIO_REST_COUNT)    // VSYNC ライン残りカウント
// 3. push(PIO_NORMAL_LINES_COUNT) // 通常ライン数 (Y レジスタ)
// 4. for _ in 0..V_NORMAL_LINES {
//        push(PIO_HSYNC_COUNT)   // 通常ライン HSYNC パルスカウント
//        push(PIO_REST_COUNT)    // 通常ライン残りカウント
//    }
// ```
//
// 合計: 3 + 2 * V_NORMAL_LINES = 225 writes/frame
//
// ## タイミング検証
//
// 各ライン合計 512 NCLK の内訳:
// - HSYNC フェーズ: overhead 2 NCLK + loop (X_h+1) NCLK = X_h + 3 NCLK
// - 残りフェーズ:   overhead 2 NCLK + loop (X_r+1) NCLK = X_r + 3 NCLK
// - ライン末尾: nop + jmp = 1 NCLK
// - 合計: X_h + X_r + 7 = 512
//
// X_h=2 → HSYNC パルス 5 NCLK, X_r=503 → 残り 506 NCLK + 末尾 1 NCLK
//
// ## PIO プログラム (28 命令 / 32 命令スロット)
//
// ```asm
// .side_set 1
//
// .wrap_target
// ; === VSYNC アクティブライン (1 ライン) ===
//     set pins, 0      side 0    ; [0]  HSYNC=0, VSYNC=0
//     pull block        side 1    ; [1]  FIFO→OSR: HSYNC カウント
//     mov x, osr        side 0    ; [2]  X = カウント
//     nop               side 1    ; [3]  アラインメント
// hsync_v0:
//     nop               side 0    ; [4]
//     jmp x-- hsync_v0  side 1    ; [5]  X+1 回ループ
//
//     set pins, 1       side 0    ; [6]  HSYNC=1, VSYNC=0
//     pull block        side 1    ; [7]  FIFO→OSR: 残りカウント
//     mov x, osr        side 0    ; [8]
//     nop               side 1    ; [9]
// rest_v0:
//     nop               side 0    ; [10]
//     jmp x-- rest_v0   side 1    ; [11]
//
// ; === 通常ラインカウントロード ===
//     pull block        side 0    ; [12] FIFO→OSR: 通常ライン数-1
//     mov y, osr        side 1    ; [13] Y = ライン数-1
//
// ; === 通常ラインループ ===
// normal_line:
//     set pins, 2       side 0    ; [14] HSYNC=0, VSYNC=1
//     pull block        side 1    ; [15] FIFO→OSR: HSYNC カウント
//     mov x, osr        side 0    ; [16]
//     nop               side 1    ; [17]
// hsync_v1:
//     nop               side 0    ; [18]
//     jmp x-- hsync_v1  side 1    ; [19]
//
//     set pins, 3       side 0    ; [20] HSYNC=1, VSYNC=1
//     pull block        side 1    ; [21] FIFO→OSR: 残りカウント
//     mov x, osr        side 0    ; [22]
//     nop               side 1    ; [23]
// rest_v1:
//     nop               side 0    ; [24]
//     jmp x-- rest_v1   side 1    ; [25]
//
//     nop               side 0    ; [26] NCLK アラインメント
//     jmp y-- normal_line side 1  ; [27] 次のライン (Y=0 で wrap)
// .wrap
// ```

// ============================================================
// Layer 3: デュアルSM ピクセル出力 PIO プログラム
// ============================================================
//
// ## アーキテクチャ
//
// SM0 (PIO0 SM0): ピクセル出力 + NCLK
// SM1 (PIO0 SM1): HSYNC/VSYNC タイミング
//
// Layer 2 の単一 SM 方式 (28命令) から、デュアル SM に分離することで:
// - SM0: 2命令のみ (autopull でピクセルデータを自動供給)
// - SM1: 19命令 (sideset なし、タイミング専用)
// - SM0 の OUT ピンで 18-bit RGB666 データを出力可能に
//
// ## SM0: ピクセル出力 + NCLK (2命令)
//
// - OUT pins: GP2-GP19 (18-bit RGB666)
// - sideset 1 pin: GP20 (NCLK)
// - autopull: threshold=18, direction=Right
//
// ```asm
// .side_set 1
// .wrap_target
//     out pins, 18  side 0    ; ピクセル出力 + NCLK LOW (LCD サンプル)
//     nop           side 1    ; NCLK HIGH
// .wrap
// ```
//
// ## SM1: HSYNC/VSYNC タイミング (19命令)
//
// - SET 2 pins: GP21 (HSYNC, bit0), GP22 (VSYNC, bit1)
// - sideset なし
// - clock divider: SM0 の 2倍 → 1 PIO cycle = 1 NCLK
//
// ```asm
// .wrap_target
// ; VSYNC active line (1 line)
//     set pins, 0            ; HSYNC=0, VSYNC=0
//     pull block
//     mov x, osr
// hsync_v0:
//     jmp x-- hsync_v0       ; X+1 回ループ
//
//     set pins, 1            ; HSYNC=1, VSYNC=0
//     pull block
//     mov x, osr
// rest_v0:
//     jmp x-- rest_v0
//
// ; 通常ラインカウントロード
//     pull block
//     mov y, osr
//
// ; 通常ラインループ
// normal_line:
//     set pins, 2            ; HSYNC=0, VSYNC=1
//     pull block
//     mov x, osr
// hsync_v1:
//     jmp x-- hsync_v1
//
//     set pins, 3            ; HSYNC=1, VSYNC=1
//     pull block
//     mov x, osr
// rest_v1:
//     jmp x-- rest_v1
//
//     jmp y-- normal_line
// .wrap
// ```
//
// ## SM1 タイミング計算 (1命令 = 1 NCLK)
//
// HSYNC フェーズ: set(1) + pull(1) + mov(1) + loop(X+1) = X + 4 NCLK
// 残りフェーズ:   set(1) + pull(1) + mov(1) + loop(X'+1) = X' + 4 NCLK
// ライン末尾:     jmp y--(1) = 1 NCLK
// 合計: X + X' + 9 = 512 → X + X' = 503
//
// HSYNC パルス 5 NCLK: X + 4 = 5 → X = 1  (FIFO value = 1)
// 残り 507 NCLK:       X' + 4 + 1 = 507 → X' = 502  (FIFO value = 502)
//
// ## Clock Divider
//
// SM0: FixedU32::<U8>::from_bits(5580)  = 21.796875 (2 PIO cycles = 1 NCLK)
// SM1: FixedU32::<U8>::from_bits(11160) = 43.59375  (1 PIO cycle  = 1 NCLK)
//
// ## ピクセルワードフォーマット (right shift, OUT_BASE=GP2)
//
// bit 0  → GP2  (B5, 青MSB)
// bit 5  → GP7  (B0, 青LSB)
// bit 6  → GP8  (G5, 緑MSB)
// bit 11 → GP13 (G0, 緑LSB)
// bit 12 → GP14 (R5, 赤MSB)
// bit 17 → GP19 (R0, 赤LSB)
//
// pixel_word = (R << 12) | (G << 6) | B

// ============================================================
// Layer 4: DMA スキャンライン転送
// ============================================================
//
// ## アーキテクチャ変更点 (Layer 3 → Layer 4)
//
// PIO プログラムは Layer 3 と同一（SM0: 2命令, SM1: 19命令）。
// 変更点はデータ供給方式のみ:
//
// Layer 3: CPU → SM0 TX FIFO (wait_push)
// Layer 4: DMA → SM0 TX FIFO (dma_push) ← NCLK停止リスク解消
//
// SM1 は CPU wait_push で駆動（225 words/frame, 低負荷）。
//
// ## DMA 転送フロー (1フレーム)
//
// 1. SM0 VSYNC DMA: 504 ワード BLACK (プレフィル 8 + DMA 504 = 512)
// 2. SM0 通常ライン DMA × 111: 各 512 ワード (107 BLACK + 400 カラー + 5 BLACK)
// 3. SM1 CPU供給: VSYNC 3 ワード + 通常ライン 2 × 111 = 225 ワード
//
// ## NCLK 停止リスク分析
//
// DMA 転送間ギャップ: ~0.3-0.7μs (CPU オーバーヘッド)
// SM0 FIFO バッファ: 8 エントリ ≈ 2.3μs
// → 全ギャップで FIFO > オーバーヘッド のため安全
