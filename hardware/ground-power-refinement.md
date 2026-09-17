# GNDベタ・電源配線ブラッシュアップ

更新日: 2026-09-17

## 採用ルール

- GNDゾーンは電源部とロジック部の両方にF.Cu/B.Cuを設け、合計4ゾーンとした。
- 全ゾーンのパッド接続をサーマルリリーフに統一した。サーマルギャップとスポーク幅はともに0.30 mm。GNDビアは電流・高周波帰路のインピーダンスを増やさないダイレクト接続のままとした。
- +13V8/-13V8は、主幹だけでなく帰還・表示LED分岐も含め、物理的に可能な全配線を1.00 mmへ拡幅した。
- J1の0.5 mmピッチ部は隣接GNDパッドとの間に1.00 mmを通せないため、パッドからのF.Cu逃げ3区間だけを0.30 mmとした。+13V8ビアを(29.3, 42.5) mmへ移し、B.Cuで両電源を直ちに反対方向へ分岐させることで、従来の0.60 mmネックは廃止した。
- -13V8のLED分岐はR7、VCPP_ADJビア、上辺の段差形状を避けて引き直し、全区間を1.00 mmとした。
- 0.60/0.30 mm（パッド径/ドリル径）の電源ビアを、空間のある主要層切替点で2本並列にした。J1直下は1本でも概算電流容量を満たし、並列化するとパッドや隣接電源へ干渉するため単独のままとした。
- C11周辺でB.CuのGND島が生じないよう、(19.8, 27.0) mmにGNDスティッチビアを追加してF.Cu/B.Cuを直結した。配線拡幅後も2本のサーマルスポークを確保するため、C11のGNDパッドだけスポーク角度を45度とした。ギャップとスポーク幅は0.30 mmのまま。

## 根拠

- [KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)はゾーンのパッド接続にsolid/thermalを選べ、サーマルギャップとスポーク幅を設定できるとしている。
- [JLCPCBのthermal relief解説](https://jlcpcb.com/blog/thermal-relief-pad-design)を参考に、はんだ付け性を優先してパッドはサーマル、ビアはダイレクト接続とした。本基板では周辺配線とのDRC成立を確認した0.30 mmギャップ/0.30 mmスポークを採用した。
- [TI SLVA959B](https://www.ti.com/lit/an/slva959b/slva959b.pdf) Table 2では、1 oz銅箔・温度上昇10 ℃の条件で12 milドリルのビアを約0.84 Aとしている。本基板の0.30 mmドリルは約11.8 milで、R7=0.47 ΩとMC34063の約330 mVしきい値から見積もるピーク約0.70 Aに対して単独でも同程度以上だが、主要な層切替は余裕を持たせて2本化した。この数値はIPC-2152ベースの目安であり、実基板の温度測定ではない。
- [onsemi AN920](https://www.onsemi.com/download/application-notes/pdf/an920-d.pdf)に基づき、大電流ループを短く太くし、電流制限の概算ピークを配線・ビア判断に用いた。

## 実装結果

- +5V並列ビア: (18.0, 27.385)/(17.2, 27.385) mm、(25.5, 37.0)/(26.3, 37.0) mm。
- +13V8並列ビア: (13.0, 42.7)/(12.2, 42.7) mm。
- -13V8並列ビア: (23.5, 41.5)/(22.7, 41.5) mm。
- +13V8は1.00 mmが24区間、-13V8は1.00 mmが17区間。1.00 mm未満はJ1パッド逃げの0.30 mm×3区間だけで、0.60 mm区間は残っていない。
- J1付近のB.Cu帰路を直線的に引き直した。+13V8は27.046 mm（6区間）から16.585 mm（3区間）へ38.7%短縮し、-13V8は11.963 mm（4区間）から9.136 mm（2区間）へ23.6%短縮した。線幅1.00 mmと部品配置は変更していない。
- 再生成には`route_power_stage.py`と`route_pico_lcd.py`を使う。未拡幅の既配線基板には`upgrade_ground_power.py`の後に`widen_13v_routes.py`を適用し、既に1.00 mm化済みの旧経路だけを更新する場合は`optimize_j1_bias_routes.py`を使う。

## 検証

- KiCad 9 ERC: 0件。
- KiCad 9 DRC: 未配線0件。短絡、クリアランス、穴クリアランス、サーマル不足、銅箔端、ダングリング、ソルダーマスクブリッジはいずれも0件。
- 残る13件は変更前と同じ非配線項目: ライブラリ不一致9件、シルク端2件、H3のkeepout 1件、シルク重なり1件。
- 今回の[変更前DRC](../review/13v-width-2026-09-17/before-drc.json)、[変更後DRC](../review/13v-width-2026-09-17/after-drc.json)、[ERC](../review/13v-width-2026-09-17/erc.json)、[移行レポート](../review/13v-width-2026-09-17/migration-report.json)を保存した。
- 銅箔確認用の[Top SVG](../review/13v-width-2026-09-17/top-copper.svg)と[Bottom SVG](../review/13v-width-2026-09-17/bottom-copper.svg)を出力した。
- J1帰路最適化の[変更前DRC](../review/j1-bias-route-optimization-2026-09-17/before-drc.json)、[変更後DRC](../review/j1-bias-route-optimization-2026-09-17/after-drc.json)、[ERC](../review/j1-bias-route-optimization-2026-09-17/erc.json)、[移行レポート](../review/j1-bias-route-optimization-2026-09-17/migration-report.json)を保存した。変更前後のDRC分類・件数は一致した。
- 最適化後の銅箔確認用[Top SVG](../review/j1-bias-route-optimization-2026-09-17/top-copper.svg)と[Bottom SVG](../review/j1-bias-route-optimization-2026-09-17/bottom-copper.svg)を出力した。

電流容量、温度上昇、電源リップル、EMIは実機未測定であり、製造前に通電評価を行う。
