# Pico 2 W / P110M Matter 取得試験

液晶を接続しない Pico 2 W に、Matter コントローラと Wi-Fi 通信を載せる独立した試験です。
既存の液晶ファームウェアと Cargo の依存関係は変更しません。

## 処理

1. USB シリアルで起動ログを出力。`START` を受信するか、起動から約64秒後に進む。
2. 設定した2.4 GHz Wi-FiへWPA2で接続。IPv4 DHCPと、MACから生成したIPv6リンクローカルを使用。
3. mDNSで同じコントローラに登録済みのノードを探し、そのままCASE認証セッションを確立する。
4. 未登録なら設定コードの短いdiscriminatorを使って登録待ちノードを探し、PASEで登録する。
5. CASEを確立し、Basic InformationとDescriptorのServerListを読み取る。
6. Electrical Power Measurement (`0x0090`) を持つendpointを選び、ActivePower (`0x0008`) を約5秒おきに読み取る。

成功時は `ACTIVE_POWER endpoint=... raw_mW=... watts=... W` がUSBログに出ます。
`null`、属性エラー、発見待ち、通信エラーは数値の取得成功と区別して記録します。
コンセントのOn/Off操作は行いません。初回の登録ではP110Mにこの試験用のMatter fabricを追加します。

## ローカル設定

`probe.example.toml` を `probe.local.toml` にコピーし、SSID、パスワード、11桁のMatter設定コードをローカルで入力してください。
設定コードは桁数・チェックディジット・passcodeの有効範囲をビルド時に確認します。
Wi-FiはWPA2の8〜63バイトのパスフレーズを使います。
Tapoアプリで生成した設定コードには有効時間があるため、実機試験直前に確認してください。
登録待ち広告がない場合はTapoアプリのMatter設定コード画面でペアリングモードを有効にします。

`controller-seed.local.bin` は初回ビルド時にOSの乱数から生成する、このコントローラ専用の秘密です。
再起動・再ビルドで同じfabricと鍵を再現するため、登録後は削除・再生成しないでください。
セッション用の乱数は各起動時のROSCから取得し、ChaCha20で生成します。

設定ファイル、鍵、依存ソース、フラッシュバックアップ、生成バイナリはGitの対象外です。
生成バイナリにも認証情報を含むので、配布しないでください。

## 依存ソース

- `project-chip/rs-matter`: `fa143d961c5838c867c9a1d382180a698e76ec51`
  - リポジトリルートの `.local/matter-probe/rs-matter` に配置。
- `embassy-rs/embassy` のWi-Fi firmware: `3a748f4cffd57311f3ced14717554f1aa983d6e4`
  - `.local/matter-probe/embassy/cyw43-firmware` に配置。
- 実行ライブラリは crates.io の Embassy RP 0.10、Net 0.9.1、CYW43 0.7。
  - Cargo.lockで試験時の依存関係を固定。

## ビルド・実行

リポジトリルートから実行します。`cargo run` は使わず、書き込み対象を必ずシリアル番号で指定してください。

```powershell
powershell -NoProfile -File experiments/pico2w-matter-probe/flash.ps1
python experiments/pico2w-matter-probe/capture.py --serial 3C10A29F7E389333 --seconds 45 --output .local/matter-probe/run.log
```

USBログ取得はPythonのpyserialを使用し、VID・PID・固有シリアル番号が一致するポートだけを開きます。
PCはビルド・書き込み・試験ログ取得に使用します。Matterデータの中継には使用しません。

`flash.ps1` はビルド後、指定シリアル番号の機体だけをUSB経由でBOOTSELに戻し、書き込み・検証・再起動します。
Windowsのpicotoolでは `load -f` が使えないため、`reboot -u -f` と `load -u -v -x` を分けます。
USB処理は割り込みexecutorで動かし、Wi-FiやMatterの処理がメインを占有してもUSB要求を処理できるようにします。
USB初期化前の障害や割り込み停止などの場合は、物理BOOTSEL操作が必要です。

## 試験上の制限

使用中のrs-matterのcommissionerは、メーカー証明書のDAC/PAA/DCL検証を未実装です。
この試験では上流の `allow_test_attestation` を明示して使います。
PASEの設定コード認証とCASEの運用証明書による暗号化通信は行いますが、製品としての証明書検証は別途必要です。
CAの鍵はローカルseedから再生成します。永続化・運用管理・時刻設定・再接続処理は量産実装ではありません。

## 実機確認の結果（2026-10-04）

**Pico 2 W単体でP110Mの電力をMatter経由で取得できた。** 液晶の実装は追加していない。

- 対象はRP2350機体 `3C10A29F7E389333`。書き込み前の4 MiBフラッシュを `.local/matter-probe/original-3C10A29F7E389333.bin` に保存し、検証済み。
- Matterを含むARM releaseビルド、実機書き込み、フラッシュ内容の検証に成功。
- `flash.ps1 -SkipBuild` でUSBからのBOOTSEL切り替え → 書き込み・検証 → 再起動を、ボタン操作なしで実機確認。
- Tapoアプリで登録待ち状態を開き、Pico自身がMatter登録を行って、endpoint 1のActivePowerを読み取った。最初の取得ログは `.local/matter-probe/run-04.log`。例: `374573 mW = 374.573 W`。
- 再書き込み・再起動後も保存したseedから同じfabricを再現し、追加登録なしでCASE認証と電力読み取りに成功。最終版の証拠は `.local/matter-probe/run-07.log`。
- 最終版ではWi-Fiの1回目の接続がタイムアウトし、切断と5秒待機後の2回目で接続に成功。最大3回の再試行と、その実機での復帰を確認。
- 約5秒間隔で `340.950 W → 344.706 W → 343.336 W` と電力値の更新を確認。
- PCはUSBログを記録するだけで、P110MとのWi-Fi通信、Matter処理、属性読み取りはすべてPico上で実行。

再起動後に取得したBasic Information:

| 属性 | P110Mから返った値 |
| --- | --- |
| VendorID | 5010 (`0x1392`) |
| ProductID | 259 (`0x0103`) |
| ProductName | Smart Wi-Fi Plug |
| HardwareVersionString | 1.0 |
| SoftwareVersionString | 1.3.0 |

MatterのSoftwareVersionStringは `1.3.0` と返った。Tapoアプリでユーザーが確認したFW `1.4.3` とは表記が異なるため、同一のバージョン番号とは扱わない。
初期のUSB停止、Wi-Fi接続失敗、二重探索による読み取り前のNotFoundは、以前の診断ログ `run-02`〜`run-06` に残している。
最終版はWPA2接続、独立したUSB処理、Wi-Fi接続の再試行、初回探索でのCASE確立を使う。

## 仕様・設定手順の根拠

- [TP-Link 日本 P110M V1の更新履歴](https://www.tp-link.com/jp/support/download/tapo-p110m/v1/): 1.3.2でMatter 1.3による電力・電力量取得を追加。ユーザー申告の1.4.3はその後の版。
- [Matter公式データモデルのElectrical Power Measurement](https://github.com/project-chip/connectedhomeip/blob/master/src/app/zap-templates/zcl/data-model/chip/electrical-power-measurement-cluster.xml): cluster `0x0090`、ActivePower `0x0008`、単位mW、nullable。
- [Tapo公式の複数コントローラ登録手順](https://www.tapo.com/en/faq/299/): Tapoアプリで設定済みの場合、Matter設定コード画面で有効時間15分のコードを生成する。
