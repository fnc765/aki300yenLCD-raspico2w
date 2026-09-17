# GNDベタ・電源配線ブラッシュアップ

更新日: 2026-09-17

## 採用ルール

- GNDゾーンは電源部とロジック部の両方にF.Cu/B.Cuを設け、合計4ゾーンとした。
- 全ゾーンのパッド接続をサーマルリリーフに統一した。サーマルギャップとスポーク幅はともに0.30 mm。GNDビアは電流・高周波帰路のインピーダンスを増やさないダイレクト接続のままとした。
- +13V8/-13V8の空間に余裕がある主幹、出力端子、出力コンデンサ、テスト端子への経路を0.60 mmから1.00 mmへ拡幅した。
- J1直下は+13V8と-13V8の中心間隔が小さく、両方を1.00 mmにするとDRC短絡になるため、F.Cuの0.30 mmヘッダネックとB.Cuの0.60 mm制約ネックを残した。主幹へ出た後は1.00 mm。
- 0.60/0.30 mm（パッド径/ドリル径）の電源ビアを、空間のある主要層切替点で2本並列にした。J1直下は1本でも概算電流容量を満たし、並列化するとパッドや隣接電源へ干渉するため単独のままとした。
- C11周辺でB.CuのGND島が生じないよう、(19.8, 27.0) mmにGNDスティッチビアを追加してF.Cu/B.Cuを直結した。

## 根拠

- [KiCad 9 PCB Editor manual](https://docs.kicad.org/9.0/en/pcbnew/pcbnew.html)はゾーンのパッド接続にsolid/thermalを選べ、サーマルギャップとスポーク幅を設定できるとしている。
- [JLCPCBのthermal relief解説](https://jlcpcb.com/blog/thermal-relief-pad-design)を参考に、はんだ付け性を優先してパッドはサーマル、ビアはダイレクト接続とした。本基板では周辺配線とのDRC成立を確認した0.30 mmギャップ/0.30 mmスポークを採用した。
- [TI SLVA959B](https://www.ti.com/lit/an/slva959b/slva959b.pdf) Table 2では、1 oz銅箔・温度上昇10 ℃の条件で12 milドリルのビアを約0.84 Aとしている。本基板の0.30 mmドリルは約11.8 milで、R7=0.47 ΩとMC34063の約330 mVしきい値から見積もるピーク約0.70 Aに対して単独でも同程度以上だが、主要な層切替は余裕を持たせて2本化した。この数値はIPC-2152ベースの目安であり、実基板の温度測定ではない。
- [onsemi AN920](https://www.onsemi.com/download/application-notes/pdf/an920-d.pdf)に基づき、大電流ループを短く太くし、電流制限の概算ピークを配線・ビア判断に用いた。

## 実装結果

- +5V並列ビア: (18.0, 27.385)/(17.2, 27.385) mm、(25.5, 37.0)/(26.3, 37.0) mm。
- +13V8並列ビア: (13.0, 42.7)/(12.2, 42.7) mm。
- -13V8並列ビア: (23.5, 41.5)/(22.7, 41.5) mm。
- +13V8/-13V8主幹は1.00 mm。低電流の帰還、LED分岐、J1直下の制約ネックは用途とクリアランスに応じて0.20～0.60 mmを維持した。
- 再現用スクリプトは`route_power_stage.py`、`route_pico_lcd.py`、既配線基板の移行は`upgrade_ground_power.py`。

## 検証

- KiCad 9 ERC: 0件。
- KiCad 9 DRC: 未配線0件。短絡、クリアランス、穴クリアランス、サーマル不足、銅箔端、ダングリング、ソルダーマスクブリッジはいずれも0件。
- 残る13件は変更前と同じ非配線項目: ライブラリ不一致9件、シルク端2件、H3のkeepout 1件、シルク重なり1件。
- [変更前DRC](review/ground-power-refinement-2026-09-17/before-drc.json)、[変更後DRC](review/ground-power-refinement-2026-09-17/after-drc.json)、[ERC](review/ground-power-refinement-2026-09-17/erc.json)、[移行レポート](review/ground-power-refinement-2026-09-17/migration-report.json)を保存した。
- 銅箔確認用の[Top SVG](review/ground-power-refinement-2026-09-17/top-copper.svg)と[Bottom SVG](review/ground-power-refinement-2026-09-17/bottom-copper.svg)、[Top 3D](review/ground-power-refinement-2026-09-17/top-3d.png)と[Bottom 3D](review/ground-power-refinement-2026-09-17/bottom-3d.png)を出力した。

電流容量、温度上昇、電源リップル、EMIは実機未測定であり、製造前に通電評価を行う。
