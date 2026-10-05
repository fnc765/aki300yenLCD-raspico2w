//! 描画部品のホストテスト (`cargo test`)

use crate::output::BmpLoader;
use crate::ui::aafont_data::{CLOCK, MEDIUM, SMALL};
use crate::ui::bmp::{BmpInfo, Resampler};
use crate::ui::color;
use crate::ui::icons;
use crate::ui::{HEIGHT, PIXELS, WIDTH};

#[test]
fn lcd_rotation_preserves_rectangular_geometry_and_pixel_conversion() {
    let src = [1u16, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    for rotate in [false, true] {
        let mut dst = [0u32; 12];
        for y in 0..3 {
            let row = crate::ui::rotation::source_row(y, 3, rotate);
            crate::ui::rotation::copy_pixels(&src[row * 4..(row + 1) * 4], &mut dst[y * 4..(y + 1) * 4], rotate, |c| u32::from(c) * 17);
        }
        let expected: Vec<_> = if rotate { src.iter().rev().map(|&c| u32::from(c) * 17).collect() } else { src.iter().map(|&c| u32::from(c) * 17).collect() };
        assert_eq!(dst.as_slice(), expected);
    }
}

#[test]
fn power_focus_preserves_status_band_and_rotates_large_and_graph_views() {
    use crate::ui::screen::Layout;
    use crate::scenario::{HistoryJson, PowerJson, Scenario};
    use crate::ui::{PIXELS, WIDTH};
    let mut sc = Scenario::default();
    sc.power = Some(PowerJson { milliwatts: Some(343300), status: "fresh".into(), age_secs: 0 });
    sc.power_now_secs = 300;
    sc.power_history = (0..=300).step_by(5).map(|t| HistoryJson { at_secs: t, milliwatts: Some((t % 100) as i64 * 1000) }).collect();
    let bg = vec![0; PIXELS];
    for expanded in [false, true] {
        sc.status.expanded = expanded;
        sc.power_display = "normal".into();
        let normal = crate::output::render_frame(&sc, &bg, 32, Layout::Glass, 0, 0);
        for mode in ["large", "graph"] {
            sc.power_display = mode.into();
            for minutes in crate::power::HISTORY_RANGES {
                sc.power_minutes = minutes;
                let frame = crate::output::render_frame(&sc, &bg, 32, Layout::Dock, 0, 0);
                let row = if expanded { 61 } else { 74 };
                assert_eq!(&frame[row * WIDTH..], &normal[row * WIDTH..]);
                assert_ne!(&frame[..row * WIDTH], &normal[..row * WIDTH]);
                sc.rotate = 180;
                let rotated = crate::output::render_frame(&sc, &bg, 32, Layout::Dock, 0, 0);
                assert!(frame.iter().eq(rotated.iter().rev()));
                sc.rotate = 0;
            }
        }
    }
    sc.power = None;
    sc.power_display = "graph".into();
    let disabled = crate::output::render_frame(&sc, &bg, 32, Layout::Dock, 0, 0);
    sc.power_display = "normal".into();
    assert_eq!(disabled, crate::output::render_frame(&sc, &bg, 32, Layout::Dock, 0, 0));
}

#[test]
fn screen_rotation_includes_every_widget_and_status_layout() {
    use crate::scenario::{Scenario, PowerJson};
    use crate::ui::screen::Layout;
    let mut sc = Scenario::default();
    sc.power = Some(PowerJson { milliwatts: Some(354_300), status: "fresh".into(), age_secs: 0 });
    let bg: Vec<_> = (0..PIXELS).map(|i| i as u16).collect();
    for layout in [Layout::Glass, Layout::Dock, Layout::Classic] {
        for expanded in [false, true] {
            sc.status.expanded = expanded;
            sc.rotate = 0;
            let normal = crate::output::render_frame(&sc, &bg, 32, layout, 0, 0);
            sc.rotate = 180;
            let rotated = crate::output::render_frame(&sc, &bg, 32, layout, 0, 0);
            assert!(normal.iter().eq(rotated.iter().rev()));
            assert_eq!(rotated[0], normal[PIXELS - 1]);
            assert_eq!(rotated[WIDTH - 1], normal[PIXELS - WIDTH]);
        }
    }
}

#[test]
fn blend_matches_reference() {
    for &(d, s) in &[(0u16, 0xffffu16), (0x1234, 0xabcd), (0xf800, 0x07e0), (0x001f, 0xffe0)] {
        for a in 0..=32u8 {
            let got = color::blend(d, s, a);
            let ch = |c: u16, sh: u32, m: u16| ((c >> sh) & m) as u32;
            for (sh, m) in [(11, 0x1f), (5, 0x3f), (0, 0x1f)] {
                let want = (ch(s, sh, m) * a as u32 + ch(d, sh, m) * (32 - a as u32)) >> 5;
                assert_eq!(ch(got, sh, m), want, "d={d:04x} s={s:04x} a={a}");
            }
        }
    }
    assert_eq!(color::to_888(0xffff), (255, 255, 255));
    assert_eq!(color::to_888(0), (0, 0, 0));
}

fn make_bmp(w: u32, h: u32, bpp: u16, top_down: bool, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let bytes = u32::from(bpp / 8);
    let row = (w * bytes + 3) & !3;
    let mut out = Vec::new();
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&(54 + row * h).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&(if top_down { -(h as i32) } else { h as i32 }).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&bpp.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for i in 0..h {
        let y = if top_down { i } else { h - 1 - i };
        let start = out.len();
        for x in 0..w {
            let [r, g, b] = f(x, y);
            out.extend_from_slice(&[b, g, r]);
            if bytes == 4 {
                out.push(0);
            }
        }
        out.resize(start + row as usize, 0);
    }
    out
}

fn decode(data: &[u8], chunk: usize) -> Vec<u16> {
    let info = BmpInfo::parse(&data[..70.min(data.len())], data.len() as u32).unwrap();
    let mut r = Resampler::new(info);
    let mut dst = vec![0u16; PIXELS];
    while let Some((off, len)) = r.next_row() {
        for c in data[off as usize..(off + len) as usize].chunks(chunk) {
            r.push(c);
        }
        r.end_row(&mut dst);
    }
    assert!(r.done());
    dst
}

#[test]
fn bmp_exact_size_both_orders() {
    let f = |x: u32, y: u32| [(x % 256) as u8, (y * 2) as u8, 200];
    for top_down in [false, true] {
        let data = make_bmp(400, 96, 24, top_down, f);
        let dst = decode(&data, 512);
        // 左上が (0, 0)。R は x、G は 2y (ディザの誤差は 1 段まで)
        for (x, y) in [(0u32, 0u32), (255, 10), (399, 95)] {
            let (r, g, _) = color::to_888(dst[y as usize * WIDTH + x as usize]);
            let [er, eg, _] = f(x, y);
            assert!((r as i32 - er as i32).abs() <= 9, "top_down={top_down} ({x},{y}) r {r} vs {er}");
            assert!((g as i32 - eg as i32).abs() <= 5, "top_down={top_down} ({x},{y}) g {g} vs {eg}");
        }
    }
}

#[test]
fn bmp_chunking_does_not_matter() {
    let data = make_bmp(333, 150, 32, false, |x, y| [(x * 7 % 256) as u8, (y * 3 % 256) as u8, ((x + y) % 256) as u8]);
    let a = decode(&data, 512);
    for chunk in [1, 3, 5, 64, 1000] {
        assert_eq!(a, decode(&data, chunk), "chunk {chunk}");
    }
}

#[test]
fn bmp_scaling_cover_and_crop() {
    // 横長 (1600×96): 高さ 96 を保ったまま 4 倍縮小 → 幅 384 は足りないので… 実際は覆う最小倍率 = 1、中央 400 列
    let data = make_bmp(1600, 96, 24, false, |x, _| if x < 600 || x >= 1000 { [255, 0, 0] } else { [0, 0, 255] });
    let dst = decode(&data, 512);
    // 切り出しは中央の 400 列 (600..1000) なので全部青
    assert!(dst.iter().all(|&c| c == color::rgb(0, 0, 255)), "center crop");
    // 大きい画像 (1200×288): 3 分の 1 に縮小、平均
    let data = make_bmp(1200, 288, 24, true, |x, y| if (x + y) % 2 == 0 { [255, 255, 255] } else { [0, 0, 0] });
    let dst = decode(&data, 512);
    let (r, _, _) = color::to_888(dst[48 * WIDTH + 200]);
    assert!((100..=160).contains(&r), "box filter average {r}");
    // 小さい画像 (100×24): 4 倍に拡大 (最近傍)
    let data = make_bmp(100, 24, 24, false, |x, _| if x < 50 { [0, 0, 0] } else { [255, 255, 255] });
    let dst = decode(&data, 7);
    assert_eq!(dst[10 * WIDTH + 199], 0);
    assert_eq!(dst[10 * WIDTH + 200], 0xffff);
    // 縦長 (96×400): 幅を 400 へ拡大し、中央を切る
    let data = make_bmp(96, 400, 24, false, |_, y| if (182..218).contains(&y) { [0, 255, 0] } else { [0, 0, 0] });
    let dst = decode(&data, 512);
    assert!(dst[48 * WIDTH + 10] == color::rgb(0, 255, 0), "portrait center band");
}

#[test]
fn bmp_rejects_unsupported() {
    let mut data = make_bmp(10, 10, 24, false, |_, _| [0, 0, 0]);
    data[28] = 8; // 8 bit
    assert!(BmpInfo::parse(&data, data.len() as u32).is_err());
    let data = make_bmp(10, 10, 24, false, |_, _| [0, 0, 0]);
    assert!(BmpInfo::parse(&data, data.len() as u32 - 10).is_err(), "truncated");
    assert!(BmpInfo::parse(b"PK\x03\x04", 4).is_err());
}

#[test]
fn samples_decode() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
    for name in ["SUNSET.BMP", "CLOUDS.BMP", "BOKEH.BMP"] {
        let mut dst = vec![0u16; PIXELS];
        BmpLoader::load_all(&dir.join(name), &mut dst).unwrap();
        assert!(dst.iter().any(|&c| c != 0), "{name}");
    }
}

#[test]
fn icons_are_rectangular() {
    let all = [
        &icons::SUN, &icons::MOON, &icons::PARTLY_SUN, &icons::PARTLY_MOON, &icons::CLOUD, &icons::FOG, &icons::DRIZZLE,
        &icons::RAIN, &icons::SNOW, &icons::THUNDER, &icons::UNKNOWN, &icons::WIFI, &icons::UP, &icons::DOWN,
        &icons::DROP, &icons::PIN,
    ];
    for icon in all {
        let w = icon.rows[0].len();
        for row in icon.rows {
            assert_eq!(row.len(), w, "{:?}", icon.rows);
            for b in row.bytes() {
                assert!(b == b'.' || icons::ink(b).is_some(), "unknown ink {}", b as char);
            }
        }
    }
    for code in [0u8, 1, 2, 3, 45, 51, 61, 71, 80, 95, 200] {
        let _ = icons::weather_icon(code, code % 2 == 0);
    }
}

/// Audit requested text extents before the canvas clips them, including extreme meter values.
/// The moving ticker intentionally enters/leaves its viewport; fixed widgets must fit completely.
#[test]
fn power_widgets_fit_without_clipping_overlap_or_off_center_values() {
    use crate::power::{self, PowerHistory, PowerTrend};
    use crate::scenario::{HistoryJson, PowerJson, Scenario};
    use crate::ui::canvas::Canvas;
    use crate::ui::screen::{self, Layout};
    let bg = vec![0; PIXELS];
    let mut issues = std::collections::BTreeSet::new();
    for mode in ["large", "graph"] {
        for expanded in [false, true] {
            for minutes in power::HISTORY_RANGES {
                for value in [Some(0), Some(50), Some(343300), Some(999900), Some(3680000),
                    Some(-3680000), Some(100000000000000), Some(i64::MAX), Some(i64::MIN), None] {
                    let mut sc = Scenario::default();
                    sc.power_display = mode.into();
                    sc.power_minutes = minutes;
                    sc.power = Some(PowerJson { milliwatts: value, status: if value.is_some() { "stale" } else { "waiting" }.into(), age_secs: 20 });
                    sc.status.expanded = expanded;
                    sc.clock.as_mut().unwrap().month = 12;
                    sc.clock.as_mut().unwrap().day = 31;
                    sc.clock.as_mut().unwrap().hour = 23;
                    sc.clock.as_mut().unwrap().minute = 59;
                    sc.clock.as_mut().unwrap().second = 59;
                    sc.weather.as_mut().unwrap().temperature = -99.9;
                    sc.weather.as_mut().unwrap().code = 99;
                    sc.message.clear();
                    sc.power_history = vec![HistoryJson { at_secs: 60, milliwatts: value }];
                    let mut history = PowerHistory::new();
                    let mut trend = PowerTrend::new();
                    history.record(60, value);
                    history.plot(60, minutes, &mut trend);
                    let text = sc.scroll_text();
                    let mut view = sc.view(0, 0, &text);
                    view.power_trend = Some(&trend);
                    let mut pixels = vec![0; PIXELS];
                    let mut canvas = Canvas::new(&mut pixels);
                    canvas.audit_layout();
                    screen::render(&mut canvas, &bg, 32, &view, Layout::Glass);
                    let cutoff = if expanded { 61 } else { 74 };
                    let regions: Vec<_> = canvas.draw_regions().iter().filter(|r| r.bounds.y0 < cutoff).collect();
                    for (i, r) in regions.iter().enumerate() {
                        let b = r.bounds;
                        let c = r.clip;
                        if b.x0 < c.x0 || b.y0 < c.y0 || b.x1 > c.x1 || b.y1 > c.y1 {
                            issues.insert(format!("{mode}/{expanded}: clipped {} {:?} inside {:?}", r.label, b, c));
                        }
                        for other in &regions[..i] {
                            let a = other.bounds;
                            if b.x0.max(a.x0) < b.x1.min(a.x1) && b.y0.max(a.y0) < b.y1.min(a.y1) {
                                issues.insert(format!("{mode}/{expanded}: {} overlaps {}", r.label, other.label));
                            }
                        }
                        // Axis labels must also stay clear of the plot starting at x=53.
                        if mode == "graph" && b.x0 == 10 && b.y0 >= 32 && b.x1 > 49 {
                            issues.insert(format!("axis label overlaps plot: {}", r.label));
                        }
                    }
                    let number = value.map(power::watts);
                    let number = number.as_ref().map(|n| n.trim_end_matches(" W")).unwrap_or("--");
                    let r = regions.iter().find(|r| r.label.as_str() == number && r.bounds.y0 < 30).unwrap();
                    let unit = regions.iter().find(|r| r.label.as_str() == "W").unwrap();
                    // Full number plus unit centered in the left panel [4,228), to one pixel.
                    assert!(((r.bounds.x0 - 4) - (228 - unit.bounds.x1)).abs() <= 1);
                }
            }
        }
    }
    assert!(issues.is_empty(), "{}", issues.into_iter().collect::<Vec<_>>().join("\n"));
}

#[test]
fn aafont_tables_consistent() {
    for font in [&CLOCK, &MEDIUM, &SMALL] {
        for g in font.glyphs {
            assert_eq!(g.alpha.len(), font.height as usize * (g.width as usize).div_ceil(2), "{}", g.ch);
        }
        // 数字は等幅 (時計が揺れない)
        let w0 = font.advance('0');
        for d in '1'..='9' {
            assert_eq!(font.advance(d), w0);
        }
    }
    assert!(HEIGHT >= CLOCK.height as usize);
}

/// Numeric readouts need visible punctuation, not just an advance width for missing glyphs.
#[test]
fn numeric_fonts_render_decimal_points_and_minus_signs() {
    use crate::ui::canvas::Canvas;
    for font in [&CLOCK, &MEDIUM, &SMALL] {
        for ch in "0123456789.-".chars() {
            let glyph = font.glyph(ch).unwrap_or_else(|| panic!("missing numeric glyph: {ch}"));
            assert!(glyph.alpha.iter().any(|&a| a != 0), "empty numeric glyph: {ch}");
        }
        for text in [".", "-"] {
            let mut pixels = vec![0; PIXELS];
            let mut canvas = Canvas::new(&mut pixels);
            canvas.aa_text(font, text, 20, 10, 0xffff);
            assert!(pixels.iter().any(|&p| p != 0), "invisible punctuation: {text}");
        }
    }
}

#[test]
fn every_layout_renders_all_states() {
    use crate::scenario::Scenario;
    use crate::ui::screen::Layout;
    let mut sc = Scenario::default();
    let bg = vec![0x7befu16; PIXELS];
    let banner = crate::scenario::BannerJson {
        url: "http://192.168.200.130/".into(),
        code: "482913".into(),
    };
    for layout in [Layout::Glass, Layout::Dock, Layout::Classic] {
        for (expanded, with_banner) in [(false, false), (true, false), (false, true), (true, true)] {
            sc.status.expanded = expanded;
            sc.banner = with_banner.then(|| banner.clone());
            // 流れる文字の設定の部分 (0.5.1〜): 無し / URL とコード (目立たせる) / Wi-Fi 接続待ち
            sc.settings = with_banner.then(|| banner.clone());
            sc.highlight = with_banner;
            sc.settings_waiting = expanded && !with_banner;
            sc.clock = None;
            sc.weather = None;
            let _ = crate::output::render_frame(&sc, &bg, 32, layout, 0, 0);
            let d = Scenario::default();
            sc.clock = d.clock;
            sc.weather = d.weather;
            for scroll in [-5000, -10, 0, 100, 400] {
                let _ = crate::output::render_frame(&sc, &bg, 16, layout, 3, scroll);
            }
        }
    }
}

/// 設定ページの案内 (0.5.0〜): 状態 3 行の行 1 に出す文字は 66 桁以内、最長の IP でも収まる
#[test]
fn settings_banner_fits() {
    use crate::ui::screen::{Banner, banner_line};
    let b = Banner {
        url: "http://255.255.255.255/",
        code: "999999",
    };
    let line = banner_line(&b);
    assert!(line.len() <= 66, "{line}");
    assert_eq!(line.as_str(), "settings: http://255.255.255.255/  code 999999");
}

/// 流れる文字 (0.5.1〜): 設定の部分があるときは 1 周 (`width`) ずらしても画面が 1 画素も変わらない
/// (= 折り返しのつなぎ目で飛ばない)。3 つの画面構成とも、文字が帯より短い / 長い / 空の場合で確かめる
#[test]
fn looped_scroll_is_seamless_at_the_join() {
    use crate::scenario::{BannerJson, Scenario, advance};
    use crate::ui::screen::Layout;
    let bg = vec![0x39e7u16; PIXELS];
    let long = "長い文字が続きます。".repeat(8);
    for message in ["", "短い文字", long.as_str()] {
        let mut sc = Scenario::default();
        sc.message = message.into();
        sc.settings = Some(BannerJson {
            url: "http://192.168.200.130/".into(),
            code: "482913".into(),
        });
        let text = sc.scroll_text();
        assert!(text.looped());
        let w = text.width();
        for layout in [Layout::Glass, Layout::Dock, Layout::Classic] {
            let (_, area_w) = layout.scroll_area(false);
            for x in [0, -1, -37, -(w / 2), -(w - 1)] {
                let a = crate::output::render_frame(&sc, &bg, 32, layout, 0, x);
                let b = crate::output::render_frame(&sc, &bg, 32, layout, 0, x - w);
                assert!(a == b, "{} x={x} w={w} len={}", layout.name(), message.len());
            }
            // 進める規則も 1 周で戻る (位置は常に (-w, area_w] に収まる)
            let mut x = 0;
            for _ in 0..(3 * w) {
                x = advance(&text, x, 1, area_w);
                assert!(x > -w && x <= area_w);
            }
        }
    }
}
