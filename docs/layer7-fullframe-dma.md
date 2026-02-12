# Layer 7: 全フレーム DMA 転送方式

## 概要

Layer 7 は SM0（ピクセル+NCLK）と SM1（HSYNC/VSYNC）の両方を独立した DMA チャネルで
一括転送する方式である。1フレームあたりの CPU 介入をゼロにし、
Layer 6 で発生していた横ジッターを根本的に解消する。

## 背景: Layer 6 の問題

Layer 6 では SM0 のピクセルデータを **1行ずつ DMA 転送** していた:

```text
SM0: DMA(行0) → CPU介入 → DMA(行1) → CPU介入 → DMA(行2) → ...
SM1: CPU wait_push で逐次供給
```

行間の CPU 介入（DMA 再設定、SM1 への wait_push）で SM0 TX FIFO がアンダーランし、
PIO が `out pins` で停止 → NCLK が一時停止 → **横方向のジッター**が発生していた。

SM0 FIFO バッファ（8エントリ ≈ 2.3μs）で吸収できる範囲を超えるギャップが
散発的に生じることが原因である。

## Layer 7 アーキテクチャ

```text
┌─────────────────────────────────────────┐
│  アプリケーション (embedded-graphics)    │
│  └── FrameBuffer (512×96 active) に描画  │
└──────────┬──────────────────────────────┘
           │ VSYNC 境界で swap
┌──────────▼──────────────────────────────┐
│  拡張フレームバッファ (512×112)           │
│  [16行 V-blank BLACK] + [96行 active]   │
│                                         │
│  DMA_CH0 ──→ PIO0 SM0 TX FIFO          │
│  57,344 ワード一括転送 (CPU介入ゼロ)     │
└─────────────────────────────────────────┘

┌─────────────────────────────────────────┐
│  SM1 タイミングバッファ (225 ワード)      │
│                                         │
│  DMA_CH1 ──→ PIO0 SM1 TX FIFO          │
│  225 ワード一括転送 (CPU介入ゼロ)        │
└─────────────────────────────────────────┘

embassy_futures::join::join(dma_ch0, dma_ch1)
→ 両 DMA を並行実行、両方完了で次フレームへ
```

### DMA 二重化パターン

```rust
// 擬似コード
loop {
    // 両 DMA を同時開始、両方完了まで待機
    embassy_futures::join::join(
        sm0.tx().dma_push(dma_ch0, &extended_fb.data, false),
        sm1.tx().dma_push(dma_ch1, &sm1_timing_buf, false),
    ).await;

    // VSYNC 境界: バッファスワップ
    if let Ok(new_front) = SWAP_CH.try_receive() {
        RETURN_CH.send(front).await;
        front = new_front;
    }
}
```

## 拡張フレームバッファ

### レイアウト

フレームバッファを 512×96 から **512×112** に拡張し、V-blank 期間の
ブランキングデータもバッファ内に含める。DMA は先頭から末尾まで一括転送する。

```text
行 0-15  (16行): V-blank 期間 — 全ピクセル BLACK (0x00000000)
行 16-111 (96行): アクティブ期間 — [107 BLACK | 400 active | 5 BLACK]

合計: 512 × 112 = 57,344 ワード
```

### メモリレイアウト（1行あたり）

V-blank 行:
```text
[0x00000000 × 512]
```

アクティブ行:
```text
[BLACK × 107] [pixel_0 ... pixel_399] [BLACK × 5]
 ← H_BACK_PORCH →  ← H_ACTIVE →   ← H_FRONT_PORCH →
```

### 定数定義

```rust
pub const LINE_WIDTH: usize = 512;       // H_TOTAL
pub const TOTAL_HEIGHT: usize = 112;      // V_TOTAL (= V_BACK_PORCH + V_ACTIVE)
pub const ACTIVE_HEIGHT: usize = 96;
pub const V_BLANK_LINES: usize = 16;     // V_BACK_PORCH

pub const EXT_FB_SIZE: usize = LINE_WIDTH * TOTAL_HEIGHT; // 57,344 ワード
```

### アプリケーション側の変更

`set_pixel(x, y)` は引き続き (0,0) 基準で操作するが、
内部オフセットに V-blank 行数を加算する:

```rust
let offset = (V_BLANK_LINES + y) * LINE_WIDTH + H_BLANK_BEFORE_ACTIVE + x;
```

## SM1 タイミングバッファ

### 構成 (225 ワード)

SM1 PIO プログラムは `pull block` でタイミングカウント値を逐次読み取る。
Layer 6 では CPU が `wait_push` で供給していたが、Layer 7 では
事前に構築した静的バッファを DMA で一括転送する。

```text
ワード 0:   SM1_HSYNC_COUNT    (= 1)    ─┐ VSYNC ライン
ワード 1:   SM1_VSYNC_REST_COUNT (= 501) ─┘
ワード 2:   SM1_NORMAL_LINES_Y (= 110)   ... 通常ライン数

ワード 3:   SM1_HSYNC_COUNT    ─┐ 通常ライン #1  (blank)
ワード 4:   SM1_REST_COUNT     ─┘
ワード 5:   SM1_HSYNC_COUNT    ─┐ 通常ライン #2  (blank)
ワード 6:   SM1_REST_COUNT     ─┘
...
ワード 223: SM1_HSYNC_COUNT    ─┐ 通常ライン #111 (active 最終行)
ワード 224: SM1_REST_COUNT     ─┘

合計: 3 + 2 × 111 = 225 ワード
```

### バッファ初期化

```rust
static SM1_TIMING_BUF: [u32; 225] = {
    let mut buf = [0u32; 225];
    // VSYNC ライン
    buf[0] = SM1_HSYNC_COUNT;       // 1
    buf[1] = SM1_VSYNC_REST_COUNT;  // 501
    // 通常ラインカウント
    buf[2] = SM1_NORMAL_LINES_Y;    // 110
    // 通常ライン × 111
    let mut i = 0;
    while i < 111 {
        buf[3 + i * 2]     = SM1_HSYNC_COUNT;  // 1
        buf[3 + i * 2 + 1] = SM1_REST_COUNT;   // 502
        i += 1;
    }
    buf
};
```

## DMA チャネル割り当て

| チャネル | 用途 | 転送元 | 転送先 | ワード数 |
|---------|------|--------|--------|---------|
| DMA_CH0 | SM0 ピクセル | 拡張フレームバッファ | PIO0 SM0 TX FIFO | 57,344 |
| DMA_CH1 | SM1 タイミング | SM1 タイミングバッファ | PIO0 SM1 TX FIFO | 225 |

## メモリ使用量

```text
拡張フレームバッファ:
  512 × 112 × 4 bytes = 229,376 bytes (224 KB)

ダブルバッファ (× 2):
  224 KB × 2 = 448 KB

SM1 タイミングバッファ:
  225 × 4 bytes = 900 bytes (≈ 1 KB)

合計: 449 KB / 520 KB SRAM
使用率: 86%
残り: 71 KB → アプリケーション + スタック用
```

### Layer 6 との比較

| 項目 | Layer 6 | Layer 7 |
|------|---------|---------|
| FB サイズ | 512×96 = 192 KB | 512×112 = 224 KB |
| ダブルバッファ合計 | 384 KB | 448 KB |
| SM1 バッファ | なし (CPU 供給) | 1 KB |
| SRAM 使用率 | 74% | 86% |
| 残り SRAM | 136 KB | 71 KB |
| DMA チャネル | 1 (CH0) | 2 (CH0 + CH1) |
| CPU 介入 / フレーム | 112 回 (行単位 DMA) | 0 回 |
| 横ジッター | あり | なし |

## PIO プログラム

PIO プログラム自体は Layer 6 と同一。変更はデータ供給方式のみ。

- **SM0** (2命令): `out pins, 18 side 1` / `nop side 0` — autopull + DMA
- **SM1** (19命令): HSYNC/VSYNC タイミング — `pull block` + DMA

SM0 の autopull threshold=18 により、TX FIFO から自動的に
32bit ワードを OSR にロードし、下位 18bit を GPIO に出力する。
DMA が 57,344 ワードを連続供給するため、FIFO アンダーランは発生しない。

## 同期

SM0 の DMA 転送 (57,344 ワード) と SM1 の DMA 転送 (225 ワード) は
`embassy_futures::join::join` で同時開始する。SM1 の方が先に完了するが、
`join` は両方の完了を待つため問題ない。

両 SM は `common.apply_sm_batch()` で同時に開始され、
PIO クロック分周比で同期しているため、フレーム内の行単位同期は
PIO プログラムのサイクル数で保証される:

- SM0: 2 PIO サイクル / NCLK → 512 NCLK / 行 → 1024 サイクル / 行
- SM1: 1 PIO サイクル / NCLK → 512 NCLK / 行 → 512 サイクル / 行
- SM1 の clk_div = SM0 の 2倍 → 実時間で同一速度
