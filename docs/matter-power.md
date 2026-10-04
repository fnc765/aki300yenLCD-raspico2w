# P110M の Matter 消費電力表示 (ticker 0.6.1)

多機能版 `ticker` に、Pico 2 W が同じ LAN の P110M から直接読む消費電力を追加した。
PC・Home Assistant・クラウドの中継は不要。時計・天気・写真・流れる文字・設定ページ・A/B OTA と共存する。
USB で検証した開発版 0.6.0 から OTA 更新できるよう、公開版は 0.6.1 とする。

## 読み取りと画面

- Descriptor の ServerList から Electrical Power Measurement (`0x0090`) の endpoint を探す。
- CASE で認証した接続から ActivePower (`0x0008`) を約 5 秒ごとに読む。値は signed mW。
- LCD は `P110M 343.3 W` のように小数 1 桁で表示する。数値に合わせて電力欄の幅を決め、正常時の状態ラベルは省く。リレーの操作は行わない。
  文字は枠内の上下中央に揃え、背景の色・透明度・縁・角丸は天気欄やテロップと同じガラス板の設定を使う。
- 取得失敗、または最後の正常取得から 15 秒超なら、最後の値を灰色の `old Ns` として表示する。
  未取得は `-- W wait`、Matter の null は `-- W n/a`。失敗や null を 0 W として表示しない。
- 設定がない場合は電力欄を出さない。不正な設定は `config`、電力 cluster がない端末は `no meter`。
- Glass / Dock / Classic と、起動中の診断表示に対応する。

## 前回の登録情報を SD に引き継ぐ

この実装は **登録済み端末の読み取り** を担当する。初回登録には
[`experiments/pico2w-matter-probe`](../experiments/pico2w-matter-probe/README.md) を使う。
そこで P110M に登録した `controller-seed.local.bin` を失わないこと。
Pico の交換や firmware 更新でも、同じ seed から同じ fabric の認証情報を再現できる。

リポジトリのルートで実行する。

```powershell
python -X utf8 scripts/prepare-matter-config.py
```

`.local/ticker-provision/` に `WIFI.TXT` と `MATTER.TXT` が作られる。**両方とも秘密情報**。
Git、GitHub の成果物、チャットに載せない。SD ルートへコピーする。
`MATTER.TXT` は次の形式 (seed は既存の登録情報から出力する)。

```text
controller_seed=<64 hexadecimal digits from the existing private seed>
device_node=0x110
```

SSID とパスワードは従来の `WIFI.TXT`、その他の設定は `TICKER.TXT` を使う。
controller node は `0x2350`、fabric ID は probe と同じ `0x50324d1100000001`。
Matter 設定コードや Tapo のログイン情報は `ticker` には不要。

## USB から SD に設定を入れる実機試験

SD を抜かずに引き継ぐ場合は、秘密情報を含むローカル専用ビルドを明示的に作る。

```powershell
./scripts/build-ticker-local.ps1 -Provision -Tbyb
./scripts/flash-ticker.ps1 -Serial <verified 16-digit chip serial> -Partition 0 `
  -Elf .local/ticker-target/thumbv8m.main-none-eabihf/release/ticker
```

- ビルド先は `.local/ticker-target/`。**この ELF/UF2 をアップロードしない**。
- 初回起動時、SD に存在しない `WIFI.TXT` / `MATTER.TXT` だけを作成する。既存ファイルを上書きしない。
- 以降は SD の情報を使う。`-Provision` なしのビルドと CI ビルドには鍵を含めない。
- flash スクリプトは識別番号で実機を限定し、全 flash のバックアップを `.local/ticker-device/` に保存する。
  指定した A/B 区画のみを書き、他方の firmware と data 区画を残す。
- USB 自動 BOOTSEL 書き込みも使える。シリアルは読取用の診断ログを出す。
  `ACTIVE_POWER raw_mW=...` と `DISPLAY_PRESENT ... frame=...` で取得・描画反映を確認できる。

## OTA・監視・メモリ

Matter は最初の OTA 確認が終了してから開始する。回復モードには追加しない。
取得の期限は 20 秒、3 回連続で失敗したら transport を破棄して mDNS / CASE から再接続する。
別タスクの `Who::Matter` は 45 秒で監視し、Wi-Fi 待機中は park する。
Matter 用 IPv6 はリンクローカルだけを設定する。HTTP / OTA の DNS は IPv4 を使い、
IPv6 の AAAA 優先や経路未設定で既存の更新確認が失敗するのを防ぐ。DHCP 完了も IPv4 の取得を待つ。
TBYB の `Round::matter` は最初の取得処理が戻るか、設定がなく実行しない場合に完了する。
`boot_sim` の Matter 段階にも Crash / Hang を注入して buy と OTA 復旧の条件を検査する。

Matter の UDP バッファは合計 12 KB。HTTPS / OTA 用 TLS バッファは従来の 1 組を維持する。
RustCrypto の CSR 生成用にヒープを 1 KB から 4 KB に増やした。
RAM を確保するため、LCD フロントを可視画素 400×96 + 各行の左右の色にした。
見えない 16 行と横余白は SRAM の繰り返し転送で補い、同じ 509×113 ワードを出力する。
FRONT / BORDER / 制御表 / 黒は合計 159,028 B (従来の FRONT は 230,068 B)。
色・解像度・表示開始位置 106 は同じ。DMA の全読取元は SRAM、走査継続は CPU / 割り込みに依存しない。
フレーム通知は CH0 の画素再ロード転送完了で、画素の CH2 は制御表転送を担当する。
CH0 の他の転送では `IRQ_QUIET` を有効にし、通知は 1 フレームに 1 回だけ。
CH3 の同期データ再ロードはタイミング FIFO の先読み分だけブランキングより早く完了するため、描画の待ち合わせには使わない。
DMA 完了通知の設定は [RP2350 データシートの IRQ_QUIET](https://datasheets.raspberrypi.com/rp2350/rp2350-datasheet.pdf) に従う。

`stack-report.py` に Matter タスクを追加し、Matter が使う ECDSA を除外対象から外した。
スタックの数値は変更後の ELF と実機の最大使用量で確認する。

### 描画更新速度

USB の `RENDER_STATS` は 5 秒ごとに、画面反映回数 (`fps_x10`) と走査回数 (`scan_fps_x10`) を別々に出す。
`600` は 60 FPS。描画時間と `present()` の待ち・コピー時間も平均 / 最大を記録する。
走査が 60 FPS でも、画面が 60 回更新されているとは限らない。

最初の実装では CH3 の通知時点が有効表示中だったため、`present()` が毎回ブランキングを逃して 4 回待ち直していた。
実機で走査 60 FPS に対して画面反映は約 12 FPS、`present()` は約 76 ms だった。
画素の再ロード完了へ通知を移し、通常表示は 60 FPS、`present()` は約 7〜10 ms に戻った。
写真の GPIO SPI 読み込み中は 30〜50 FPS へ一時的に下がる。これは従来の同期 SD 処理の制限で、
通常表示が常時 12 FPS に落ちる不具合と区別する。SD の速度・読み込み予算・画像処理は従来のまま。

## 実機確認 (2026-10-04)

- 液晶・SD 付き Pico 2 W で、秘密情報を埋め込まない TBYB 版から SD の登録情報を読み、
  CASE / endpoint 1 / ActivePower / LCD の `present()` まで確認した。
  起動から約 6 分間観測し、電力は約 5 秒ごとに更新、フレーム数も増加し続けた。
- fnc765 の OTA 参照先から HTTPS の 404 (Release 未作成) を受け、`proved=true`。
  起動約 72 秒で `BOOT_BUY Bought`、180 秒を超えても巻き戻されないことを確認した。
- 表示の目視確認は「電力が見えていて、欠け・ちらつきなし」との利用者確認。
- 確定後に USB から再起動し、再書き込みせず同じ版で電力取得を再開した。
  この起動では mDNS の再試行を経て、起動約 80 秒で最初の電力を取得した。初回取得までの時間は LAN の応答に依存する。
- 全 bin、ticker / wifi_ota の TBYB 版、clippy、ホスト 64 件、画面シミュレータ 14 件が通った。
  描画速度修正後の ticker の空きスタック 62,400 B、最深経路 + overhead 25,832 B、margin 36,568 B。
  イメージは 1,808,612 B で A/B 区画の 1,966,080 B に収まる。
- 配布用 ELF / bin に既存の seed・Wi-Fi パスワードが無いことと、bin が実機試験版と同一であることを確認した。
- 電力欄の正常時ラベルを省き、218 px の固定幅から文字に合わせた幅へ縮めた。
  同期修正後の USB ログで通常表示 60 FPS、約 5 秒ごとの電力取得・画面反映、TBYB の buy を確認した。
  写真読み込み中の一時的な低下は上記の制限として残る。

## 対応範囲

公開版 0.6.1 の配布前検証では、全 bin、ticker / wifi_ota の TBYB 版、clippy、ホスト 64 件、画面 14 件が通った。
ローカルの ticker.bin は 1,807,980 B。空きスタック 62,400 B、最深経路 + overhead 25,656 B、margin 36,744 B。

rs-matter は probe で検証した revision `fa143d961c5838c867c9a1d382180a698e76ec51` に固定。
P110M で確認した Tapo firmware は 1.4.3。Matter の SoftwareVersionString は 1.3.0 だった。
Tapo アプリの電力機能があっても、他の機種・firmware が Matter で電力 cluster を公開するとは限らない。
この firmware は初回登録や DAC/PAA/DCL 検証を実装しない。登録実験の制約は probe の README を参照。

プレビューは `tools/ui-sim/scenarios/matter-*.json`。CI の ui-preview でも生成される。
物理 LCD の見え方、Wi-Fi 切断復帰、実際の OTA 更新は、それぞれの実機試験と区別する。
