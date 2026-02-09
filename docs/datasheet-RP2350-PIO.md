# RP2350 (Raspberry Pi Pico 2W) PIO 仕様まとめ

## 基本仕様

| 項目 | 値 |
|------|-----|
| MCU | RP2350 |
| ボード | Raspberry Pi Pico 2W |
| CPU | Dual-core Arm Cortex-M33 / RISC-V Hazard3 (150MHz) |
| SRAM | 520 KB |
| Flash | 4 MB (外部) |
| GPIO | 30本 (4本アナログ対応) |
| 無線 | Wi-Fi + Bluetooth (CYW43439) |

## PIO (Programmable I/O) 仕様

| 項目 | RP2350 | 参考: RP2040 |
|------|--------|-------------|
| PIO ブロック数 | 3 | 2 |
| ステートマシン数 | 12 (3×4) | 8 (2×4) |
| 命令メモリ | 32命令/ブロック | 32命令/ブロック |
| FIFO バッファ | 4ワード × 32bit / SM | 4ワード × 32bit / SM |
| クロック | システムクロック (最大150MHz) | 最大133MHz |
| GPIO 接続 | 全30ピン | 全30ピン |

### PIO の特長

- **1サイクル1命令**: 確定的なタイミング制御
- **GPIO 同時駆動**: 複数ピンを1命令で同時制御
- **DMA 連携**: FIFO ↔ DMA で CPU 負荷なしのデータ転送
- **分周器**: 整数+小数分周でピクセルクロック生成可能
- **サイドセット**: データ出力と同時に制御信号を操作

### DMA 仕様

| 項目 | 値 |
|------|-----|
| DMA チャネル数 | 16 |
| 転送幅 | 8/16/32 ビット |
| チェーン | 転送完了時に別のDMAを自動起動 |
| リングバッファ | アドレスのラップアラウンドサポート |
| 最大帯域 | 32bit/cycle @ 150MHz = 600 MB/s |

## LCD/VGA 駆動実績

### pico_scanvideo_dpi ライブラリ

Raspberry Pi 公式の VGA/DPI 出力ライブラリ。

| 解像度 | ピクセルクロック | 用途 |
|--------|-------------|------|
| 640×480 @ 60Hz | 25.175 MHz | VGA標準 |
| 320×240 @ 60Hz | 6.3 MHz | QVGA |
| 480×272 @ 60Hz | 約9.0 MHz | HVGA (小型LCD) |

### 標準構成

```
SM0: HSYNC/VSYNC/DE タイミング生成
SM1: RGB データ出力 (ピクセルデータ)
DMA: フレームバッファ → PIO FIFO に自動転送
```

### GPIO 割り当て例

```
GPIO  0-5  : R[5:0] (赤 6bit)
GPIO  6-11 : G[5:0] (緑 6bit)
GPIO 12-17 : B[5:0] (青 6bit)
GPIO 18    : DCLK (ピクセルクロック)
GPIO 19    : HSYNC
GPIO 20    : VSYNC
GPIO 21    : DE
─────────────────────────────
合計: 22 GPIO
```

> Pico 2W では無線モジュール (CYW43439) が一部 GPIO を使用するため、
> 利用可能な GPIO 数に注意が必要。

## 公式サンプルコード

| サンプル | 概要 | リポジトリ |
|---------|------|-----------|
| scanvideo_minimal | 基本テストパターン | pico-playground |
| test_pattern | カラーバー表示 | pico-playground |
| demo1/demo2 | 複雑な描画 | pico-playground |
| mandelbrot | リアルタイム計算 | pico-playground |
| flash_stream | フラッシュからビデオ出力 | pico-playground |

## 参考リンク

- [Raspberry Pi Pico 2 製品ページ](https://www.raspberrypi.com/products/raspberry-pi-pico-2/)
- [RP2350 データシート](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf)
- [pico-extras (scanvideo)](https://github.com/raspberrypi/pico-extras)
- [pico-playground (サンプル)](https://github.com/raspberrypi/pico-playground)
- [Pico C SDK ドキュメント](https://www.raspberrypi.com/documentation/microcontrollers/c_sdk.html)
- [VGA リファレンス設計 (KiCAD)](https://datasheets.raspberrypi.com/rp2040/VGA-KiCAD.zip)
- [RP2040 ハードウェア設計ガイド](https://datasheets.raspberrypi.com/rp2040/hardware-design-with-rp2040.pdf)
