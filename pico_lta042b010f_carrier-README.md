# Raspberry Pi Pico 2W / LTA042B010F KiCad reviewed draft

KiCad 9.0.1で開ける、銅箔配線前のキャリア基板ドラフトです。回路図は、MC34063の電源回路を実線配線で表し、Pico／LCD間の多本数RGB666信号だけをネット名で整理しています。

## 成果物

- `pico_lta042b010f_carrier.kicad_pro` — KiCadプロジェクト
- `pico_lta042b010f_carrier.kicad_sch` — 更新済み回路図
- `pico_lta042b010f_carrier.kicad_pcb` — 120 mm x 70 mm、部品配置済み・未配線
- `pico_lta042b010f_carrier.kicad_sym` / `sym-lib-table` — 埋め込み記号とローカル記号ライブラリ設定
- `pico_lta042b010f_carrier-kicad-native-reviewed.pdf` — KiCad純正のA3 PDF
- `pico_lta042b010f_carrier-kicad-native-reviewed-preview.png` — 上記PDFをそのままラスタライズした確認画像
- `pico_lta042b010f_carrier-pcb-3d-top.png` — KiCad純正PCB 3Dトップ表示
- `pico_lta042b010f_carrier-reviewed-erc.txt` / `pico_lta042b010f_carrier-reviewed-drc.txt` — 検査結果

## 置き換えた部品とフットプリント

| Ref | 回路記号／値 | 採用フットプリント |
|---|---|---|
| U1 | Raspberry Pi Pico 2W | `Module:RaspberryPi_Pico_W_SMD_HandSolder` |
| J1 | LTA042B010F 36P FFC | `Connector_FFC-FPC:TE_3-1734839-6_1x36-1MP_P0.5mm_Horizontal` |
| U3 | MC34063AD | `Package_SO:SOIC-8_3.9x4.9mm_P1.27mm` |
| R7 | 0.47 Ω current sense | `Resistor_SMD:R_2512_6332Metric_Pad1.40x3.35mm_HandSolder` |
| L1 | 150 µH | `Inductor_SMD:L_Sunlord_MWSA1204S-150` |
| D4/D6/D7 | 1N5819 Schottky | `Diode_SMD:D_SMA` |
| R8/R9/R10/R11/R16 | 0805抵抗 | `Resistor_SMD:R_0805_2012Metric` |
| C7 | 470 pF | `Capacitor_SMD:C_0805_2012Metric` |
| D5/D8 | LED | `LED_SMD:LED_0805_2012Metric` |
| C6/C8/C9/C11 | 極性コンデンサ | `Capacitor_THT:CP_Radial_D5.0mm_P2.00mm` |
| RV1 | 10 kΩトリマ | `Potentiometer_THT:Potentiometer_Bourns_3296W_Vertical` |
| J2/J3 | 2P電源コネクタ | `Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical` |

Picoのフットプリントは、Raspberry Pi公式資料のPico W/Pico 2 W共通の40ピン・2.54 mm配置を前提に、KiCad公式フットプリントライブラリのSMD手はんだ版を採用しています。Pico 2 Wのアンテナ側は基板端から離して配置しています。FFCの接触面、ピン1方向、実際のケーブル型番は現物照合が必要です。

## 検査結果

- ERC: **エラー0、警告0**
- 回路図ネットリスト: +5V、+13V8、-13V8、GND、Pico/LCD RGB・同期信号、MC34063周辺の接続を確認済み
- PCB DRC: **未配線85件**。これは「アートワーク前」の状態によるものです。配置由来のコートヤード重なり、基板端クリアランス、文字高さは解消済みです。残る37件は、未配線以外ではシルク重なり／シルク下の銅箔警告18+8件と、KiCadライブラリ部品を基板ファイルへ展開した際の照合警告11件です。

U3は、SwEをGNDへ意図的に接続するこの回路でERCを成立させるため、埋め込み記号の該当ピンをpassiveとして扱っています。そのためプロジェクトでは、この意図的な記号差分だけ`lib_symbol_mismatch`を無視しています。実機のMC34063AD品種とデータシートを再確認してください。

## 電源回路の前提

- +5 V入力をMC34063ADで昇圧し、+13.8 Vを生成
- C9、D6、D7、C11で反転チャージポンプを構成し、-13.8 Vを生成
- Pico 3V3_OUTをLCDのDVDD/AVDD、+5 VをGVDD、+13.8 VをVGON、-13.8 VをVSSへ接続
- VCPP_ADJは10 kΩトリマで調整
- 実機投入前に、ダイオード極性、電解コンデンサ極性、インダクタの飽和電流、+13.8 V/-13.8 Vの負荷時電圧・リップルを確認

PCB Editorで **Update PCB from Schematic (F8)** を実行し、現物に合わせて外形・コネクタ位置・Picoアンテナ周辺・電源／GND配線を確定した後、銅箔配線と最終DRCへ進みます。

## 参考資料

- [Raspberry Pi Pico-series公式ドキュメント](https://www.raspberrypi.com/documentation/microcontrollers/pico-series.html)
- [Raspberry Pi Pico 2 W公式データシート](https://datasheets.raspberrypi.com/picow/pico-2-w-datasheet.pdf)
- [KiCad公式フットプリントライブラリ](https://gitlab.com/kicad/libraries/kicad-footprints/)
- [KiCad公式Pico W SMD手はんだフットプリント](https://gitlab.com/kicad/libraries/kicad-footprints/-/blob/master/Module.pretty/RaspberryPi_Pico_W_SMD_HandSolder.kicad_mod?ref_type=heads)
- [onsemi MC34063A公式データシート](https://www.onsemi.com/download/data-sheet/pdf/mc34063a-d.pdf)
- [Diodes Incorporated 1N5819公式ページ](https://www.diodes.com/part/view/1N5819)
- [pol: ESP32-S3で秋月300円液晶を動かす](https://pol.hateblo.jp/entry/2023/11/27/001524)
- [なる研: LTA042B010F解析](https://naruken.cweb.tk/labo/naruken/lta042b010f/)
