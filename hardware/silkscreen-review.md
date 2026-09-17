# シルクスクリーン確認

更新日: 2026-09-17

## 実施内容

- 上面シルクへ基板名 `PICO2W-LTA042B010F` を追加した。
- 上面シルクへ `REV A  2026-09-17` を追加した。既存のリビジョン表記がなかったため、初回表記をREV Aとした。
- H1/H4のリファレンスを基板外から内側へ移動した。
- D6の極性表示 `K` をC6の外形円と重ならない位置へ移動した。

## 配置

- 基板名: (37.5, 14.0) mm、文字高1.20 mm、線幅0.18 mm。
- リビジョン・日付: (37.5, 16.0) mm、文字高1.00 mm、線幅0.15 mm。
- H1リファレンス: (4.0, 8.0) mm。
- H4リファレンス: (96.0, 8.0) mm。
- D6 `K` 表示: (10.85, 39.0) mm。

## 検証

- KiCad 9 ERC: 0件。
- KiCad 9 DRC: 変更前13件、変更後10件、未配線0件。
- `silk_over_copper`: 0件。パッドおよび露出銅箔との重複なし。
- `silk_overlap`: 0件。シルク同士の重複なし。
- `silk_edge_clearance`: 0件。基板端からのはみ出しなし。
- 残る10件は従来からのライブラリ関連9件とH3のRF keepout 1件。
- 31個の部品配置、184個のパッド形状・ネット、403個の配線・ビア、4個のGNDゾーンは変更していない。
- [変更前DRC](../review/silkscreen-review-2026-09-17/before-drc.json)、[変更後DRC](../review/silkscreen-review-2026-09-17/after-drc.json)、[ERC](../review/silkscreen-review-2026-09-17/erc.json)、[移行レポート](../review/silkscreen-review-2026-09-17/migration-report.json)を保存した。
- 目視確認用に[シルク単独SVG](../review/silkscreen-review-2026-09-17/top-silkscreen.svg)、[PNG](../review/silkscreen-review-2026-09-17/top-silkscreen.png)、[銅箔重ね合わせSVG](../review/silkscreen-review-2026-09-17/top-silkscreen-copper.svg)、[PNG](../review/silkscreen-review-2026-09-17/top-silkscreen-copper.png)を保存した。

実基板の印刷品質と文字の視認性は、基板製造後に確認する。
