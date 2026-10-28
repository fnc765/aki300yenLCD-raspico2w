# ファームウェア セキュリティ監査 (静的解析) 2026-10-28

対象: `codex/release-ticker-matter-0-6-1` @ 9999e86 (`Cargo.toml` 1.0.0)。
方法: ソース静読のみ (ビルド・実機・fuzz なし)。3 領域 (設定サーバ / OTA・ブート / ネット・周辺) を並列監査し、主要指摘は本体コードで再確認した。
「受容済」= `docs/ota-design.md` §9、`docs/settings-server.md` §4、`docs/wifi-ota.md` §6 に既知の制限として明記されているもの。

## 0. 総括

- CRITICAL 1 (受容済だが攻撃連鎖は未記載)、HIGH 1 (新規)、MEDIUM 6、LOW 13。
- メモリ破壊・認証なしで確実に panic させる入力は見つからなかった (web / RSS / JSON / BMP / SNTP / wifi.txt)。
- 実質的なリスクは設計上のもの: TLS 無検証 + manifest 無署名 + イメージ種別無検査。この 3 点が重なると「LAN の攻撃者が任意ファームを永続化し、OTA 経路ごと奪う」まで進む。

## 1. CRITICAL

### C1. TLS 無検証 + manifest 無署名 → LAN 上の MITM で任意ファームが永続化 (受容済)
- `src/ota/app.rs:177` `TlsVerify::None`、`src/ota/manifest.rs` `is_newer_than_current` (version > CURRENT のみ)。
- 連鎖: DNS/ARP 偽装 → `manifest.json` に `version:"65535.0.0"` + 自作 bin の sha256 → 端末は書込・検証・FLASH_UPDATE・buy まで自動。
- buy は止まらない: `BuyGate` (`src/boot_policy.rs:386-398`) は起動したイメージ自身が判定。悪意イメージは `explicit_buy` を自前で呼ぶ / TBYB を付けない / WDT を止めるのいずれでも通る。`ota_proved` は 404/5xx でも Proved (`classify_check`)。
- 事後: 以後の正規版は「新しくない」ので更新不能。回復は BOOTSEL 手動のみ。悪意イメージは data 区画の Wi-Fi 平文パスワード、SD の `MATTER.TXT` seed も読める。
- 同じ構造は誤公開 (例 9.9.9) でも発生。強制ダウングレード手段なし。
- 対策: Ed25519 署名 (version/bin/size/sha256 全体、JSON パース前に検証、公開鍵埋込)。TLS 検証 + ホスト許可リスト + https 限定は第 3 段階でまとめて。

## 2. HIGH

### H1. ダウンロードしたイメージの IMAGE_DEF / TBYB / 版数を一切検査しない (新規) — **対応済 (同日)**
- 対応: `src/ota/image.rs` (commit 前に IMAGE_DEF を解釈し TBYB / RP2350 EXE / 版数一致を検査)、`ota::OTA_BIN_NAMES` (`ticker.bin` のみ)、`scripts/check-image-def.py` (CI / `make-manifest.sh` の assert)。ホストテスト `tools/ticker-tests/src/image_tests.rs`。
- `src/ota/app.rs:1085-1137`、`src/ota/slot.rs` `commit_first_sector`: 検査は SHA-256 一致のみ。
- Release には `out/*.bin` 全部が付く (`release.yml:116`、`build.yml:290`): `ticker-plain.bin` / `wifi_ota-plain.bin` (TBYB なし) / `layer*.bin` / `sd_bmp_viewer.bin`。公開 `.sha256` をそのまま使えるので **悪意コード不要、manifest 書換だけ** で OTA を持たない非 TBYB イメージを入れられる。
- 非 TBYB イメージは buy 不要で居座り、「OTA 到達保証」が効かない。先頭セクタ commit → readback の電源断窓も TBYB 前提が崩れる。
- manifest の version と IMAGE_DEF の version 不一致も受理。
- CI: `build.yml:202` `picotool info ... | grep -E 'version|tbyb' || true` → TBYB 欠落を検出しない。
- 対策: commit 前に RAM 上の先頭セクタを解析 (IMAGE_DEF 有効・TBYB=1・version == manifest・> CURRENT)。`bin` を `ticker.bin` 許可リストに。CI の `|| true` を外し assert。

## 3. MEDIUM

### M1. manifest の入れ子 JSON でスタック溢れ → OTA 確認のたびリセット (OTA 到達保証が破れる)
- `src/ota/manifest.rs:79` `serde_json_core::from_slice`。未知フィールドは `deserialize_ignored_any` (`serde-json-core-0.6.0/src/de/mod.rs:740-`) で `[`/`{` を再帰、深さ制限なし。
- `MANIFEST_MAX=512` に `"x":[[[[...` 約 250 段。空きスタック余裕 ≈16 kB (docs/ticker.md) を超えると MSPLIM HardFault。
- 回復モード・フォールバック先の旧版も同じ parser → 攻撃者がいる間は OTA に到達できない (消えれば復帰)。実機未検証。
- 対策: 署名検証を JSON パース前に。または `[`/`{` 個数をバイト走査で先に制限 (≤ 8 など)。

### M2. manifest 経由のフラッシュ摩耗 DoS、検証前に旧版スロットを消す構造
- `src/ota/app.rs:1086-1087` `writer.begin()` が先頭セクタ消去 → 攻撃者が「新 version + わざと sha 不一致 bin」を流すと毎回 消去→書込→失敗→`invalidate` (消去 2 回)。
- バックオフ (`app.rs:777-779`) は上限 600 s だが、UpToDate を 1 回挟むと `OTA_BACKOFF_MIN` に戻る (`app.rs:737-741`) → 約 120 s 周期維持可。10 万サイクル定格で約 70 日。
- 攻撃中は他方スロット常時無効 → 実行側が壊れると BOOTSEL 回復。`DOWNLOAD_TIMEOUT` 300 s の slowloris で占有可。
- 対策: 失敗回数を永続化し上限。UpToDate でバックオフを戻さない。

### M3. 「入れない版」(blocked) と Rejected 近道の悪用
- `src/bin/ticker.rs:2314`、`src/boot_policy.rs:129-140`、`app.rs:1054-1070, 788`。
- 悪意イメージが buy 後に `fallback_marker` を自分で書くと、旧正規ファームが `blocked = max(from)` を data 区画に保存 → major ≤ 15 の全版が OTA 不可 (data 区画消去まで)。
- 他方スロットの現行内容 (過去の公開版) の sha を manifest に載せると DL なしで Rejected → 10 分毎に FLASH_UPDATE 再起動 (`REJECTED_RETRY_DELAY`)。非 TBYB の旧版へは版数が低くても起動 → 一時的ダウングレード + 10 分周期の稼働妨害。
- `version_word` (`boot_policy.rs:33` `(major<<16)|(minor*100+patch)`) は patch ≥ 100 / minor ≥ 656 で桁重なり。`Version::parse` は制限していない。
- 対策: 署名導入時に version/sha/size + 単調カウンタを署名対象に。blocked の信頼範囲見直し。

### M4. 認証なし GET で LAN の誰でも取得タスク・SD・スライドショーを占有 (DoS)
- `src/web/server.rs:646-686` `stream_image`、`:1035-1048` `lock_sd`/`hold_slideshow`、`src/bin/ticker.rs` ≈1651 `serve` 分岐。
- `GET /img/X.BMP` を送り応答を読まない → TCP 窓 1 kB で 5 s タイムアウトを刻み最大 30 s 粘れる。`GET /api/images` 連打で `HOLD_UNTIL_MS` 更新し続けスライドショー停止。
- `serve` は NTP/天気/文字より先 → 3 取得が飢餓。OTA 確認は先頭なので進む (リセット・OTA 阻害には至らず)。単一ソケットなので正規利用者も「応答なし」。実機未検証。
- 対策: /img の総時間短縮。認証なし GET は `hold_slideshow` を呼ばない。

### M5. 認証なし USB class request で BOOTSEL に入れる → flash 丸読み (Wi-Fi 平文パスワード)
- `src/usb_reset.rs:83-97`: Class/Interface/request 0x01 で `reset_to_usb_boot(0,0)`。picotool 互換の標準挙動だが認証なし。
- 接続先 PC が侵害されていれば、ボタン操作なしで `picotool save` → data 区画の SSID/パスワード (`src/persist.rs`、平文、CRC のみ)。セキュアブート/OTP flash ロック未使用 (`docs/ota-design.md:23`)。
- 対策 (任意): reset interface を外すビルド。資格情報の暗号化は鍵の置き場所が課題。

### M6. Matter の `controller_seed` = fabric ルート鍵。SD と provision 済 ELF に平文
- `src/matter/mod.rs:101-130` (seed から RCAC/ICAC/コントローラ鍵を決定論生成)、`src/ticker/power.rs:145-165`、`build.rs:30-45` + `src/provision.rs` (`TICKER_PROVISION_DIR` で定数埋込)。
- seed 保有者は任意 NOC を発行し P110M の fabric 管理者として CASE → **商用電源の On/Off**。SD 紛失 / provision 済 UF2 漏洩で成立。本ファーム自身は `read` のみ発行。
- 確認済: `.local/` は gitignore、履歴に実鍵なし、CI Release ビルドは provision を使わない。
- 対策: SD と provision 済バイナリの物理管理。P110M 側 ACL で本コントローラを Read 相当に絞る。

## 4. LOW

| # | 場所 | 内容 |
|---|---|---|
| L1 | `src/ota/manifest.rs:83` | `bin` の禁止は空と `/` のみ。`..` `?` `#` `\` 空白 制御文字を許す。ホスト固定なのでパス脱出不可だがクエリ注入・CR/LF 注入の余地 (生 CR/LF 通過は未検証)。`[A-Za-z0-9._-]` に絞る |
| L2 | `src/ota/http.rs:84-95` | 3xx を任意ホスト・`http://` 降格でも追従 (上限 3)。TLS 検証導入時にホスト許可リスト + https 限定必須 |
| L3 | `src/ticker/config.rs` `message_url`、`src/bin/ticker.rs:899` | 認証済み者 / SD 書換者が LAN 内 SSRF (ルータの GET 型管理 URL 等)。応答 512 B が LCD に出る。RFC1918 / 自身 IP を拒否する |
| L4 | `src/bin/ticker.rs:1593`、`app.rs:176` | アクセスコードと TLS seed が `RoscRng` 1 回分。暗号 TRNG ではない (品質未検証)。RP2350 TRNG へ |
| L5 | `src/ticker/config.rs:425` vs `src/web/server.rs:918` | `seen=[false;16]` に対し `SETTING_KEYS` 17 個。17 キー同時送信で末尾に重複行が増え、`CONFIG_MAX` 到達で 413。`[false;17]` に |
| L6 | `src/web/server.rs:354-363` | `POST /api/show-code` は認証なし。`show_settings=0` でも LAN の誰でも LCD にコードを出させられる (ネットには出ない) |
| L7 | `src/bin/ticker.rs:174-196`、`src/ticker/usb_access.rs` | USB `WEB_ACCESS\n` でコードが返る。接続 PC 上のプロセスが取得可。docs の受容リスト未記載 |
| L8 | `src/web/server.rs:309`、`auth.rs` | `as_millis() as u32` が 49.7 日で巻戻り。直前 15 分にロックが掛かると約 49 日間 429 固定 (条件極狭) |
| L9 | `src/web/server.rs:263` | `head_len + body_len as usize` が dev プロファイルのみ `Content-Length: 4294967295` で panic (release は wrap → 413/400)。`saturating_add` に |
| L10 | `src/ticker/sntp_net.rs:77`、`sntp.rs` | SNTP 無認証、送信元・Origin timestamp 未確認。影響は時計・ニュース鮮度フィルタ・FAT 日時のみ (TLS/OTA は時刻非依存)。48 B 超の応答で容易に妨害 |
| L11 | `src/ticker/config.rs:536-552` | `parse_tz` 下限未検査。`tz=--1e9` で release は wrap、dev は panic。入力は SD / 認証済 Web |
| L12 | `src/sdcard.rs:93-114`、`slideshow.rs` | u32 ms 期限が 49.7 日で折返し → SD 転送が一時 `BusTimeout` (可用性のみ、未検証) |
| L13 | `src/bin/ticker.rs:1031-1034` | 天気取得は TLS 失敗 1 回で再起動まで平文 HTTP 固定 (TLS 無検証なので劣化は小) |

情報: `host_is_board` が `+92` / `010` を受理 (悪用不可)、`news_topics` の `#` は再読込でコメント扱い (整合性のみ)、`debug_crash` は SD 書換者のみ・回復モードで復帰。

## 5. CI / リリース
- `inputs.version` / `inputs.ref` は env 経由 + 正規表現検証で注入なし。シークレット露出なし。
- `release.yml` の `ref` が任意ブランチ/コミット可、Environment 承認なし → write 権限者が未レビュー枝を OTA 配布可。
- アクションは tag 参照のみ (SHA ピン留めなし)、`dtolnay/rust-toolchain@stable` はブランチ参照。prebuilt picotool はチェックサム未検証 (UF2 のみに影響)。`cargo build` に `--locked` なし、`cargo audit`/`deny` なし。

## 6. 問題なかった点 (要約)
- HTTP パーサ: ヘッダ 2 kB / Content-Length 10 桁 / chunked 501 / Host 重複 400 / パイプライン Close。panic 経路なし。
- 認証: カウンタ global、固定長比較、ロック中は正コードも拒否、コードは JSON のどこにも出ない。Host/Origin/Sec-Fetch-Site は全ルート適用。
- フォーム / ticker.txt: `valid_value` が全キーで CR/LF 拒否、キー allowlist 17、`debug_crash`/`sdfast` 不可。改行注入不可。
- アップロード / 削除 / `/img`: サイズ 115254 固定、ヘッダ全検査、8.3 英数字正規化、`is_bmp_name` で `.BMP` 以外不可 → `WIFI.TXT`/`TICKER.TXT` の読み書き不可。
- JSON エンコーダのエスケープ、`index.html` は動的値を `textContent`、CSP あり (`'unsafe-inline'` は弱点だが注入点なし)。
- Wi-Fi パスワード: defmt / USB log に出ない。SD 読込 256 B 打切り + 長さ検査。
- RSS / Open-Meteo / BMP / SNTP の各 parser: 境界・範囲・UTF-8 境界すべて検査済。`unsafe` に外部長さ由来なし。
- OTA slot: 書込先は manifest size ≤ 区画サイズで実行中スロット・data 区画・PT に及ばない。readback SHA は先頭セクタ含む全域。
- Matter: コミッショニング窓を開くコードなし → テスト用パスコード 20202021 / テスト DAC でも LAN から commission 不可 (将来窓を開くなら要変更)。

## 7. 未検証
1. embedded-tls 0.18 / reqwless 0.14 が悪意応答で panic しないか (HEAP 4 kB の RSA 経路の alloc 失敗含む)。
2. reqwless URL parser の悪意 `message_url` / `Location`。
3. rs-matter の UDP 5540 / 5353 認証前パース、`max-sessions-3` 枯渇。
4. embedded-sdmmc 0.10 の細工 FAT (クラスタ連鎖ループ) → WDT 頼み。
5. ephemeral ポートの乱数性。
6. M1 / M4 の実機再現。

## 8. 優先順位
1. 署名 (C1 / M1 / L1 を同時に緩和)。
2. H1 のイメージ検査 + `bin` 許可リスト + CI assert (署名とは独立に必要)。
3. M2 失敗回数永続化 / バックオフ非リセット、M3 blocked 見直し。
4. M4 /img 時間短縮、L5 / L9 / L11 は 1 行修正。
5. `release.yml` ref 固定 or Environment 承認、アクション SHA ピン留め。
