//! Wi-Fi OTA (第 2 段階): GitHub Release から manifest / bin を取得し、他方の A/B 区画へ書き込む
//!
//! 設計は docs/ota-design.md §5、使い方は docs/wifi-ota.md。
//!
//! - [`manifest`]: `manifest.json` の解釈と semver 比較
//! - [`http`]: reqwless (HTTPS, TLS 1.3) でのダウンロード。GitHub の 302 リダイレクトを自前で追う
//! - [`slot`]: 書き込み先区画の決定、セクタ消去 / ページ書き込み、SHA-256、ATRANS を通さない読み戻し
//! - [`app`]: 上の 3 つを組み合わせた実行部 (TBYB のウォッチドッグ延長と自己診断、join / DHCP の接続管理、
//!   定期的な更新確認と書き込み、LCD 用の状態文字列)。`wifi_ota` と `ticker` が共用し、bin 側は描画だけを持つ。

pub mod app;
pub mod http;
pub mod image;
pub mod manifest;
pub mod slot;

use core::fmt::Write as _;

use heapless::String;

/// 更新元リポジトリ (`gh api repos/... --jq .full_name` の正規名)。SD カード等からは読まない。
pub const REPO: &str = "fnc765/aki300yenLCD-raspico2w";
/// Release アセット名
pub const MANIFEST_NAME: &str = "manifest.json";
/// manifest の `bin` として受け付ける Release アセット名 (1.0.1〜)。
///
/// Release には全 bin の `.bin` / `.sha256` が付くので、名前を自由にすると manifest の書き換えだけで
/// OTA を持たない bin (`layer*.bin`、`sd_bmp_viewer.bin`) を入れられる。OTA の経路 (早期ウォッチドッグ、
/// 生存確認、回復モード) を持つ `ticker` だけを許す。`wifi_ota` を再び配るときは SKILL.md §2.2 の条件を
/// 満たしてからここに足す。
pub const OTA_BIN_NAMES: &[&str] = &["ticker.bin"];
/// GitHub の署名付きリダイレクト先 URL (release-assets.githubusercontent.com は JWT 付きで 1 kB を超える)
pub const URL_MAX: usize = 2048;

/// `url` を `https://github.com/<REPO>/releases/latest/download/<name>` にする。
///
/// 0.4.1〜: 値で返さず呼び出し側のバッファへ直接書く。`String<URL_MAX>` (2 kB) を値で返すと、
/// 呼び出しごとに main タスクの poll のスタックフレームに 2 kB の一時領域が取られていた
/// (0.4.0 は 5 か所で計 10 kB、docs/ticker.md「スタック」)。
pub fn set_latest_asset_url(url: &mut String<URL_MAX>, name: &str) {
    url.clear();
    let _ = write!(url, "https://github.com/{}/releases/latest/download/{}", REPO, name);
}

/// OTA の失敗理由 (LCD に短く表示する)
#[derive(Clone, Copy, Debug, PartialEq, Eq, defmt::Format)]
pub enum OtaError {
    /// DNS 解決失敗
    Dns,
    /// TCP 接続 / 切断 / タイムアウト
    Network,
    /// TLS ハンドシェイク失敗
    Tls,
    /// 応答ヘッダが受信バッファ (`HTTP_HEADER_SIZE`) に収まらない (reqwless `BufferTooSmall`)
    HttpHeaderTooLong,
    /// 応答の構文が解釈できない (reqwless `Codec`: ステータス行 / ヘッダ / chunked 本文)
    HttpCodec,
    /// 3xx なのに `Location` が無い、http(s) でない、または URL として解釈できない
    HttpRedirect,
    /// その他の HTTP クライアント内部エラー (reqwless `AlreadySent` など)
    HttpProtocol,
    /// 200 / 302 / 404 以外の HTTP ステータス
    HttpStatus(u16),
    /// リダイレクトが多すぎる
    TooManyRedirects,
    /// Location ヘッダが `URL_MAX` を超えた
    LocationTooLong,
    /// manifest.json の JSON / 版数 / sha256 が解釈できない
    Manifest,
    /// manifest の `bin` が [`OTA_BIN_NAMES`] に無い (1.0.1〜)
    BinNotAllowed,
    /// manifest の size が 0 か区画より大きい
    BadSize,
    /// 受信バイト数が manifest の size と違う
    SizeMismatch,
    /// 受信データの SHA-256 が manifest と違う
    ShaMismatch,
    /// 受信したイメージの先頭セクタが TBYB の RP2350 実行イメージでない、または版数が manifest と違う (1.0.1〜)
    Image(image::ImageError),
    /// 書き込み後の読み戻し SHA-256 が manifest と違う
    ReadbackMismatch,
    /// フラッシュ消去 / 書き込みエラー
    Flash,
    /// パーティションテーブルが読めない / A/B 構成でない
    NoPartitionTable,
    /// 自区画が特定できない、または他方区画が無い
    NoTarget,
    /// 全体タイムアウト
    Timeout,
}

impl OtaError {
    /// LCD 用の短い名前
    pub fn label(self) -> &'static str {
        match self {
            OtaError::Dns => "DNS failed",
            OtaError::Network => "network error",
            OtaError::Tls => "TLS failed",
            OtaError::HttpHeaderTooLong => "HTTP header too long",
            OtaError::HttpCodec => "HTTP parse error",
            OtaError::HttpRedirect => "bad redirect URL",
            OtaError::HttpProtocol => "bad HTTP response",
            OtaError::HttpStatus(_) => "HTTP status",
            OtaError::TooManyRedirects => "too many redirects",
            OtaError::LocationTooLong => "redirect URL too long",
            OtaError::Manifest => "bad manifest.json",
            OtaError::BinNotAllowed => "bin not allowed",
            OtaError::BadSize => "bad image size",
            OtaError::SizeMismatch => "size mismatch",
            OtaError::ShaMismatch => "sha256 mismatch",
            OtaError::Image(e) => e.label(),
            OtaError::ReadbackMismatch => "flash readback mismatch",
            OtaError::Flash => "flash error",
            OtaError::NoPartitionTable => "no partition table",
            OtaError::NoTarget => "no target slot",
            OtaError::Timeout => "timeout",
        }
    }
}

// `image` はホスト (tools/ticker-tests) でも使うので defmt に依存しない。ログ用の表示だけここで付ける。
impl defmt::Format for image::ImageError {
    fn format(&self, f: defmt::Formatter<'_>) {
        defmt::write!(f, "{}", self.label());
    }
}
