# ハードウェア接続設計: Pico 2W ↔ LTA042B010F

## PIO のピン制約（最重要）

### OUT ピンは連続でなければならない

PIO の `out pins, N` 命令は、**ベースピン（OUT_BASE）から連続する N 本の GPIO** にデータを出力する。
不連続なピンへの同時出力は不可能。

したがって、**RGB データピン（18本 or 12本）は連続した GPIO に配置する必要がある**。

| ピンタイプ | ベース | 最大本数 | 連続必須 | ラップ対応 |
|-----------|--------|---------|---------|----------|
| OUT       | 0-31   | 32      | **はい** | あり (mod 32) |
| SET       | 0-31   | **5**   | **はい** | あり |
| SIDESET   | 0-31   | **5**   | **はい** | あり |

### SIDESET でクロック生成

SIDESET ピンはデータ出力と同時に制御可能なため、**NCLK（ドットクロック）を SIDESET で生成**するのが最適。
SIDESET ピンは OUT ピンとは独立したベースを持つ。

### SET ピンで同期信号制御

HSYNC/VSYNC は SET ピン（最大5本）で制御するか、別のステートマシンの OUT ピンで制御する。

## Pico 2W GPIO 制約

### 利用不可 GPIO（CYW43439 Wi-Fi/BT が使用）

| GPIO | CYW43439 での用途 | 状態 |
|------|-----------------|------|
| GP23 | WL_REG_ON（電源ON） | **使用不可** |
| GP24 | SPI DATA/IRQ | **使用不可** |
| GP25 | SPI CS | **使用不可** |
| GP29 | SPI CLK + ADC3 | **使用不可** |

### 利用可能 GPIO 一覧

| GPIO | 物理ピン | デフォルト機能 | LCD用途での制約 |
|------|---------|--------------|----------------|
| GP0 | 1 | UART0 TX | デバッグ用に温存推奨 |
| GP1 | 2 | UART0 RX | デバッグ用に温存推奨 |
| GP2 | 4 | 汎用 | LCD データに使用可能 |
| GP3 | 5 | 汎用 | LCD データに使用可能 |
| GP4 | 6 | I2C0 SDA | LCD データに使用可能 |
| GP5 | 7 | I2C0 SCL | LCD データに使用可能 |
| GP6 | 9 | 汎用 | LCD データに使用可能 |
| GP7 | 10 | 汎用 | LCD データに使用可能 |
| GP8 | 11 | 汎用 | LCD データに使用可能 |
| GP9 | 12 | 汎用 | LCD データに使用可能 |
| GP10 | 14 | 汎用 | LCD データに使用可能 |
| GP11 | 15 | 汎用 | LCD データに使用可能 |
| GP12 | 16 | 汎用 | LCD データに使用可能 |
| GP13 | 17 | 汎用 | LCD データに使用可能 |
| GP14 | 19 | 汎用 | LCD データに使用可能 |
| GP15 | 20 | 汎用 | LCD データに使用可能 |
| GP16 | 21 | SPI0 RX | LCD データに使用可能 |
| GP17 | 22 | SPI0 CSn | LCD データに使用可能 |
| GP18 | 24 | SPI0 SCK | LCD データに使用可能 |
| GP19 | 25 | SPI0 TX | LCD データに使用可能 |
| GP20 | 26 | 汎用 | LCD 制御に使用可能 |
| GP21 | 27 | 汎用 | LCD 制御に使用可能 |
| GP22 | 29 | 汎用 | LCD 制御に使用可能 |
| GP26 | 31 | ADC0 | LCD用途では不向き（ADC温存推奨） |
| GP27 | 32 | ADC1 | LCD用途では不向き |
| GP28 | 34 | ADC2 | LCD用途では不向き |

**連続GPIO の最大範囲**: GP0〜GP22 = **23本連続**（CYW43439のGP23で途切れる）

## ピン割り当て案

### 案A: RGB666 フル接続（推奨）

UART0 を温存し、RGB666 全18ビット + 制御信号を配線。

```
┌─────────────────────────────────────────────────────┐
│ PIO SM0 (ピクセルデータ出力)                         │
│   OUT_BASE  = GP2                                   │
│   OUT_COUNT = 18  (GP2〜GP19)                       │
│   SIDESET_BASE = GP20                               │
│   SIDESET_COUNT = 1  (NCLK)                         │
│                                                     │
│ PIO SM1 (タイミング制御)                             │
│   SET_BASE = GP21                                   │
│   SET_COUNT = 2  (HSYNC, VSYNC)                     │
└─────────────────────────────────────────────────────┘

GPIO 割り当て:
  GP0      : UART0 TX (デバッグ)
  GP1      : UART0 RX (デバッグ)
  ─── PIO OUT ピン (連続18本) ───
  GP2      : B5 (青 MSB)      → LCD Pin 8
  GP3      : B4               → LCD Pin 9
  GP4      : B3               → LCD Pin 10
  GP5      : B2               → LCD Pin 11
  GP6      : B1               → LCD Pin 12
  GP7      : B0 (青 LSB)      → LCD Pin 13
  GP8      : G5 (緑 MSB)      → LCD Pin 15
  GP9      : G4               → LCD Pin 16
  GP10     : G3               → LCD Pin 17
  GP11     : G2               → LCD Pin 18
  GP12     : G1               → LCD Pin 19
  GP13     : G0 (緑 LSB)      → LCD Pin 20
  GP14     : R5 (赤 MSB)      → LCD Pin 22
  GP15     : R4               → LCD Pin 23
  GP16     : R3               → LCD Pin 24
  GP17     : R2               → LCD Pin 25
  GP18     : R1               → LCD Pin 26
  GP19     : R0 (赤 LSB)      → LCD Pin 27
  ─── PIO SIDESET (1本) ───
  GP20     : NCLK              → LCD Pin 2
  ─── PIO SET (2本) ───
  GP21     : HSYNC             → LCD Pin 4
  GP22     : VSYNC             → LCD Pin 5
  ─── CYW43439 (使用不可) ───
  GP23-25  : Wi-Fi/BT
  ─── 空きピン ───
  GP26     : ADC0 (VCPP_ADJ 出力用 or センサー)
  GP27     : ADC1 / I2C1 SCL (拡張用)
  GP28     : ADC2 / I2C1 SDA (拡張用)
  GP29     : CYW43 (使用不可)

空きGPIO: GP26, GP27, GP28 = 3本
```

### 案B: RGB565 接続（GPIO節約）

RGB の最下位ビット（R0, B0）を省略して16ビット + 制御信号。

```
┌─────────────────────────────────────────────────────┐
│ PIO SM0 (ピクセルデータ出力)                         │
│   OUT_BASE  = GP2                                   │
│   OUT_COUNT = 16  (GP2〜GP17)                       │
│   SIDESET_BASE = GP18                               │
│   SIDESET_COUNT = 1  (NCLK)                         │
│                                                     │
│ PIO SM1 (タイミング制御)                             │
│   SET_BASE = GP19                                   │
│   SET_COUNT = 2  (HSYNC, VSYNC)                     │
└─────────────────────────────────────────────────────┘

GPIO 割り当て:
  GP0      : UART0 TX
  GP1      : UART0 RX
  ─── PIO OUT ピン (連続16本) ───
  GP2      : B4 (青 MSB, 5bit) → LCD Pin 9
  GP3      : B3               → LCD Pin 10
  GP4      : B2               → LCD Pin 11
  GP5      : B1               → LCD Pin 12
  GP6      : B0 (青 LSB)      → LCD Pin 13
  GP7      : G5 (緑 MSB, 6bit)→ LCD Pin 15
  GP8      : G4               → LCD Pin 16
  GP9      : G3               → LCD Pin 17
  GP10     : G2               → LCD Pin 18
  GP11     : G1               → LCD Pin 19
  GP12     : G0 (緑 LSB)      → LCD Pin 20
  GP13     : R4 (赤 MSB, 5bit)→ LCD Pin 23
  GP14     : R3               → LCD Pin 24
  GP15     : R2               → LCD Pin 25
  GP16     : R1               → LCD Pin 26
  GP17     : R0 (赤 LSB)      → LCD Pin 27
  ─── 固定ピン ───
  LCD B5 (Pin 8)  → GND 固定
  LCD R5 (Pin 22) → GND 固定
  ─── PIO SIDESET ───
  GP18     : NCLK              → LCD Pin 2
  ─── PIO SET ───
  GP19     : HSYNC             → LCD Pin 4
  GP20     : VSYNC             → LCD Pin 5
  ─── 空きピン ───
  GP21, GP22, GP26, GP27, GP28 = 5本

空きGPIO: 5本 → I2C, SPI, センサー等に使用可能
```

### 案C: RGB444 簡易接続（最小ピン数）

RGB各4ビット = 12本 + 制御3本 = 15本。

```
┌─────────────────────────────────────────────────────┐
│ PIO SM0 (ピクセルデータ出力)                         │
│   OUT_BASE  = GP2                                   │
│   OUT_COUNT = 12  (GP2〜GP13)                       │
│   SIDESET_BASE = GP14                               │
│   SIDESET_COUNT = 1  (NCLK)                         │
│                                                     │
│ PIO SM1 (タイミング制御)                             │
│   SET_BASE = GP15                                   │
│   SET_COUNT = 2  (HSYNC, VSYNC)                     │
└─────────────────────────────────────────────────────┘

GPIO 割り当て:
  GP0      : UART0 TX
  GP1      : UART0 RX
  ─── PIO OUT (連続12本) ───
  GP2-5    : B[3:0]  → LCD Pin 10-13
  GP6-9    : G[3:0]  → LCD Pin 17-20
  GP10-13  : R[3:0]  → LCD Pin 24-27
  ─── 固定ピン ───
  LCD B[5:4] (Pin 8-9)   → GND 固定
  LCD G[5:4] (Pin 15-16) → GND 固定
  LCD R[5:4] (Pin 22-23) → GND 固定
  LCD VCPP_ADJ (Pin 29)  → 0V (Hi-Z)
  ─── 制御 ───
  GP14     : NCLK (sideset)   → LCD Pin 2
  GP15     : HSYNC (set)      → LCD Pin 4
  GP16     : VSYNC (set)      → LCD Pin 5
  ─── 空きピン ───
  GP17-22, GP26-28 = 9本

空きGPIO: 9本 → 大量の拡張余地
```

## PIO プログラム設計上の注意

### NCLK は立ち下がりエッジでサンプル

LTA042B010F は NCLK の**立ち下がりエッジ**でデータをサンプリングする。
したがって、PIO プログラムでは以下の順序が必要:

```
1. RGB データを GPIO に出力 (out pins, N)
2. NCLK を HIGH にする (sideset 1) ← データは安定している
3. NCLK を LOW にする (sideset 0) ← ここでLCDがサンプル
```

PIO プログラム例（1ピクセル出力）:
```
.side_set 1
.wrap_target
    out pins, 18  side 0  ; データ出力 + NCLK=LOW (LCD がサンプル)
    nop           side 1  ; NCLK=HIGH (データ安定待ち)
.wrap
```

> **注意**: 上記は最も単純な例。実際には HSYNC/VSYNC のブランキング期間制御が必要。

### VSYNC のサンプリングタイミング

VSYNC は HSYNC 開始から **98クロック後** の NCLK 立ち下がりでサンプリングされる。
したがって、VSYNC を任意のタイミングでアサートしても無視される。

タイミング制御 SM で正確なカウントが必要:
```
HSYNC H→L → 98 NCLK後に VSYNC を L にする → LCD が認識
```

### 複数ステートマシンの同期

- SM0（データ出力）と SM1（タイミング制御）は同じ PIO ブロック内に配置
- `pio_enable_sm_mask_in_sync()` で同時起動
- RP2350 では `NEXTPREV_SM_ENABLE/DISABLE` で隣接PIO間の同期も可能
- SM 間の wait/irq で同期ポイントを設定

### DMA 設定

```
DMA Channel 0:
  ソース: フレームバッファ (SRAM)
  デスト: PIO SM0 TX FIFO
  転送幅: 32bit
  転送数: (400 × 18bit) / 32bit = 225 ワード/ライン × 96 ライン
  チェーン: → DMA Channel 1

DMA Channel 1:
  ソース: スキャンラインアドレステーブル
  デスト: DMA Channel 0 の読み取りアドレス
  転送数: 96 エントリ（各ライン先頭アドレス）
  チェーン: → DMA Channel 0 (ループ)
```

## 電源接続

### 全体回路図

```
                    ┌──────────────┐
  USB 5V ───────────┤ VBUS         │
                    │              │
  3.3V ◄────────────┤ 3V3(OUT)     │
  (LCD DVDD/AVDD)   │              │
                    │ GP2-GP19 ────┼──── LCD RGB データ (18本)
                    │ GP20 ────────┼──── LCD NCLK
                    │ GP21 ────────┼──── LCD HSYNC
                    │ GP22 ────────┼──── LCD VSYNC
                    │              │
                    │ Pico 2W      │
                    └──────┬───────┘
                           │ GND
                           │
  ┌────────────────────────┼────────────────────────┐
  │                        │                        │
  ▼                        ▼                        ▼
LCD Pin 3,7,14,         ±12V DC-DC              LCD GND
30,32,34,36 (GND)     ┌────────────┐
                       │ +12V → LCD Pin 35 (VGON)
                       │ -12V → LCD Pin 33 (VSS)
                       │ +12V → CCFL インバーター
                       └────────────┘

LCD Pin 1 (TEST)   → GND (通常表示)
LCD Pin 6          → GND (L固定)
LCD Pin 21 (DVDD)  → 3.3V (Pico 3V3_OUT)
LCD Pin 28 (AVDD)  → 3.3V (Pico 3V3_OUT)
LCD Pin 31 (GVDD)  → 5V (VBUS) ※推奨。3.3Vでも動作
LCD Pin 29 (Vo)    → 10kΩ VR → 0〜3V (コントラスト調整)
```

### 電源レール一覧

| 電源 | 電圧 | 供給元 | 接続先 |
|------|------|--------|--------|
| 3.3V | 3.3V | Pico 3V3_OUT (最大300mA) | LCD DVDD (Pin 21), AVDD (Pin 28) |
| 5V | 5V | USB VBUS | LCD GVDD (Pin 31) |
| +12V | +12V | DC-DC 昇圧 | LCD VGON (Pin 35), CCFL インバーター |
| −12V | −12V | DC-DC 反転 | LCD VSS (Pin 33) |

### 3.3V 供給の注意

Pico 2W の 3V3_OUT は最大 **300mA**。
LCD ロジック部の消費電流は通常 10-50mA 程度なので十分だが、
他のセンサー等も接続する場合は総消費電流に注意。

## FPC コネクタ接続

### 推奨: 秋月 FPC DIP化基板 (110187)

0.5mm ピッチ 40ピン FPC を 2.54mm ピッチのピンヘッダーに変換。
ブレッドボードやユニバーサル基板での試作に最適。

### FPC ケーブルの向き

FPC コネクタの挿入方向（表裏）に注意。
ピン番号が逆転する場合があるため、テスターで導通確認すること。

## 配線チェックリスト

### 信号線（必須）
- [ ] GP2-GP7 → LCD B[5:0] (Pin 8-13)
- [ ] GP8-GP13 → LCD G[5:0] (Pin 15-20)
- [ ] GP14-GP19 → LCD R[5:0] (Pin 22-27)
- [ ] GP20 → LCD NCLK (Pin 2)
- [ ] GP21 → LCD HSYNC (Pin 4)
- [ ] GP22 → LCD VSYNC (Pin 5)

### 固定ピン（必須）
- [ ] LCD Pin 1 (TEST) → GND
- [ ] LCD Pin 6 → GND

### 電源（必須）
- [ ] LCD Pin 21 (DVDD) → 3.3V
- [ ] LCD Pin 28 (AVDD) → 3.3V
- [ ] LCD Pin 31 (GVDD) → 5V (or 3.3V)
- [ ] LCD Pin 35 (VGON) → +12V
- [ ] LCD Pin 33 (VSS) → −12V
- [ ] LCD Pin 3, 7, 14, 30, 32, 34, 36 (GND) → GND
- [ ] CCFL インバーター → DC 12V + バックライト接続

### 調整（推奨）
- [ ] LCD Pin 29 (Vo) → 10kΩ VR (0〜3V)
- [ ] VR1 (基板背面) → フリッカー調整

### デバッグ（推奨）
- [ ] GP0 (UART TX) → USB-Serial アダプタ RX
- [ ] GP1 (UART RX) → USB-Serial アダプタ TX

## 参考情報

### PIO の制約まとめ

1. **OUT ピンは連続必須** — RGB データは連続 GPIO に配置
2. **SIDESET は OUT と独立** — NCLK 生成に最適
3. **SET は最大5本** — HSYNC/VSYNC 制御に使用
4. **全PIOブロックが全GPIOにアクセス可能** — RP2350A (Pico 2W) では PIO0/1/2 いずれも GP0-29 にアクセス可能
5. **複数SMの同時起動** — `pio_enable_sm_mask_in_sync()` で確定的なタイミング
6. **SM間の同期** — wait/irq 命令で SM0（データ）と SM1（タイミング）を同期

### scanvideo_dpi との違い

| 項目 | scanvideo_dpi (VGA) | 本プロジェクト (LTA042B010F) |
|------|--------------------|-----------------------------|
| 解像度 | 640×480, 320×240 | 400×96 |
| カラー | RGB555 (16bit) | RGB666 (18bit) |
| 同期 | HSYNC + VSYNC | HSYNC + VSYNC (DE なし) |
| クロック | DCLK 出力 (立ち上がり) | **NCLK 立ち下がりサンプル** |
| ライブラリ | pico_scanvideo_dpi | カスタム PIO プログラム |

### 情報源

- [RP2350 PIO API (pico-sdk)](https://github.com/raspberrypi/pico-sdk/blob/master/src/rp2_common/hardware_pio/include/hardware/pio.h)
- [Pico 2W ボード定義 (pico-sdk)](https://github.com/raspberrypi/pico-sdk/blob/master/src/boards/include/boards/pico2_w.h)
- [pico_scanvideo_dpi ソース](https://github.com/raspberrypi/pico-extras/blob/master/src/rp2_common/pico_scanvideo_dpi/scanvideo.c)
- [なる研 - LTA042B010F 解析](http://naruken.cweb.tk/labo/lta042b010f/)
- [xcrosgs2wy - 秋月LCD解析](https://xcrosgs2wy.web.fc2.com/akilcd/)
