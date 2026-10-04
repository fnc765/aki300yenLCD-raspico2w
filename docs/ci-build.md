# CI ビルド（GitHub Actions）と UF2 の入手方法

`.github/workflows/build.yml` が、push ごとに全 `[[bin]]` を
`thumbv8m.main-none-eabihf` 向けに `cargo build --release` し、
picotool で ELF → UF2（`--family rp2350-arm-s`）に変換します。

本線は [fnc765/aki300yenLCD-raspico2w](https://github.com/fnc765/aki300yenLCD-raspico2w) です。
ワークフローの Release 公開先は `github.repository`、認証は `github.token` を使うため、
fnc765 側に push すると同じリポジトリでビルド・成果物保存・Release 公開を行います。
独自の Secrets は不要です。リポジトリの Actions 設定では利用する Actions を許可してください。
Release ジョブにはワークフロー内で `contents: write` を指定しています。
`release` / `build` の Run workflow は、ワークフローを既定ブランチに取り込んでから利用できます。

RP2350 向けのビルドは Rust のシンボル形式を `v0` に揃えます（`.cargo/config.toml`）。
スタック検査がタスク名・型情報を読めるようにするためです。
検査でタスクやスタック境界を検出できなければ、余裕があると判定せずエラーにします。

`experiments/pico2w-matter-probe` は独立した実機試験プロジェクトで、このワークフローの対象外です。
試験の設定・鍵はローカルに保存します。手順は [試験 README](../experiments/pico2w-matter-probe/README.md) を参照してください。

ファームウェアの OTA 取得先と既定の文字メッセージ取得先も fnc765 側です。
既に書き込まれている Droplet 版は従来の取得先を使い続けるため、移行版の書き込みが必要です。
SD の `ticker.txt` に `message_url=` を明記している場合は、その値も更新してください。
過去の Actions 実行履歴や Release アセットは Git のマージでは移りません。

## UF2 の入手先

- **任意の push / ブランチ**: Actions タブ → 該当の `build` 実行 → Artifacts の
  `firmware-<commit SHA>` をダウンロード（zip 内に `<bin名>.elf` / `<bin名>.uf2` / `commit.txt`）。
  保持期間は 14 日です。
- **リリース**: `v*` 形式のタグ（例 `v0.1.0`）を push すると、同名の GitHub Release が
  自動作成され、全 bin の UF2 が添付されます。こちらは無期限に残ります。
  タグを手で打たずに Actions → `release` → Run workflow（`version` = `Cargo.toml` の版数）でも
  同じ Release を作れます（`.github/workflows/release.yml`、詳細は [wifi-ota.md §3](wifi-ota.md#3-更新を配る-開発者側)）。
- **手動実行**: Actions → `build` → Run workflow。`bin` 入力に bin 名を入れると
  その 1 つだけをビルドします（既定は `sd_bmp_viewer`）。

## 書き込み方法

- **picotool（ELF / UF2 どちらでも可）**: BOOTSEL を押しながら USB 接続し、
  `picotool load -f -v -x -t elf sd_bmp_viewer.elf`（UF2 なら `-t uf2`）。
  `-f` で実行中の基板を強制的に BOOTSEL へ、`-v` で検証、`-x` で書き込み後に再起動します。
- **ドラッグ＆ドロップ**: BOOTSEL で接続すると現れる `RP2350` ドライブに `.uf2` をコピーするだけで
  書き込み・再起動されます。picotool のインストールは不要です。
