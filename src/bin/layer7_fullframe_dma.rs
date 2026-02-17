//! Layer 7: 全フレーム一括DMA転送方式
//!
//! SM0（ピクセル+NCLK）とSM1（HSYNC/VSYNC）をそれぞれ別DMAチャネルで
//! フレーム全体を一括転送し、行間のCPU介入を排除してジッターを解消する。
//!
//! # 合格基準
//! - [ ] SM0/SM1 両DMAを MULTI_CHAN_TRIGGER で完全同時起動
//! - [ ] 行間の CPU 介入がない (ジッタフリー)
//! - [ ] テキスト・図形が正しく表示される
//! - [ ] ティアリングがない

#![no_std]
#![no_main]

use core::ptr::addr_of_mut;
use core::sync::atomic::{AtomicU32, Ordering};
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::peripherals::*;
use embassy_rp::pio::program::pio_asm;
use embassy_rp::pio::{
    Config, Direction, FifoJoin, InterruptHandler, Pio, ShiftConfig, ShiftDirection,
};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Instant;
use embedded_graphics::geometry::Size;
use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::Rgb666;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use fixed::FixedU32;
use fixed::types::extra::U8;
use pico2w_300yen_lcd::lcd::framebuffer::*;
use pico2w_300yen_lcd::lcd::timing::*;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

// ============================================================
// SM1 フレームデータ (static 配置)
// ============================================================

/// SM1 の 1 フレーム分タイミングデータ (225 ワード)
static SM1_FRAME_DATA: [u32; SM1_FRAME_SIZE] = sm1_frame_data();

// ============================================================
// ペリフェラル構造体 (TaskFn 16引数制限の回避)
// ============================================================

/// display_task に渡すペリフェラル一式
struct DisplayPeripherals {
    pio0: Peri<'static, PIO0>,
    pin2: Peri<'static, PIN_2>,
    pin3: Peri<'static, PIN_3>,
    pin4: Peri<'static, PIN_4>,
    pin5: Peri<'static, PIN_5>,
    pin6: Peri<'static, PIN_6>,
    pin7: Peri<'static, PIN_7>,
    pin8: Peri<'static, PIN_8>,
    pin9: Peri<'static, PIN_9>,
    pin10: Peri<'static, PIN_10>,
    pin11: Peri<'static, PIN_11>,
    pin12: Peri<'static, PIN_12>,
    pin13: Peri<'static, PIN_13>,
    pin14: Peri<'static, PIN_14>,
    pin15: Peri<'static, PIN_15>,
    pin16: Peri<'static, PIN_16>,
    pin17: Peri<'static, PIN_17>,
    pin18: Peri<'static, PIN_18>,
    pin19: Peri<'static, PIN_19>,
    pin20: Peri<'static, PIN_20>,
    pin21: Peri<'static, PIN_21>,
    pin22: Peri<'static, PIN_22>,
    dma_ch0: Peri<'static, DMA_CH0>,
    dma_ch1: Peri<'static, DMA_CH1>,
}

// ============================================================
// ダブルバッファ管理
// ============================================================

/// フレームバッファ A (BSS 配置、ゼロ初期化)
static mut FB_A_DATA: FrameBuffer = FrameBuffer::new();

/// フレームバッファ B (BSS 配置、ゼロ初期化)
static mut FB_B_DATA: FrameBuffer = FrameBuffer::new();

/// main → display_task: 描画完了した back バッファの所有権を送信
static SWAP_CH: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 1> = Channel::new();

/// display_task → main: 使用済み front バッファの所有権を返却
static RETURN_CH: Channel<CriticalSectionRawMutex, &'static mut FrameBuffer, 1> = Channel::new();

// ============================================================
// 計測統計 (lock-free)
// ============================================================

/// main 描画処理時間（CPUのみ）[ms] 最新値
static DRAW_CPU_MS_LATEST: AtomicU32 = AtomicU32::new(0);

/// main 描画処理時間（CPUのみ）[ms] 最大値
static DRAW_CPU_MS_MAX: AtomicU32 = AtomicU32::new(0);

/// main フレームループ全体時間（描画+待機）[ms] 最新値
static FRAME_LOOP_MS_LATEST: AtomicU32 = AtomicU32::new(0);

/// main フレームループ全体時間（描画+待機）[ms] 最大値
static FRAME_LOOP_MS_MAX: AtomicU32 = AtomicU32::new(0);

/// display_task フレーム境界区間（swap判定+DMA再起動）時間 [us] 最新値
static DISPLAY_BOUNDARY_SECTION_US_LATEST: AtomicU32 = AtomicU32::new(0);

/// display_task フレーム境界区間（swap判定+DMA再起動）時間 [us] 最大値
static DISPLAY_BOUNDARY_SECTION_US_MAX: AtomicU32 = AtomicU32::new(0);

const HUD_X: i32 = 230;
const HUD_LINE1_Y: i32 = 12;
const HUD_LINE2_Y: i32 = 24;
const HUD_UPDATE_INTERVAL_FRAMES: u32 = 8;

fn saturating_u64_to_u32(v: u64) -> u32 {
    if v > u32::MAX as u64 {
        u32::MAX
    } else {
        v as u32
    }
}

fn update_max_atomic(max: &AtomicU32, value: u32) {
    let mut observed = max.load(Ordering::Relaxed);
    while value > observed {
        match max.compare_exchange_weak(observed, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next_observed) => observed = next_observed,
        }
    }
}

fn write_u32_decimal<const N: usize>(mut value: u32, buf: &mut [u8; N]) -> &str {
    let mut idx = N;
    if value == 0 {
        idx -= 1;
        buf[idx] = b'0';
    } else {
        while value > 0 {
            idx -= 1;
            buf[idx] = b'0' + (value % 10) as u8;
            value /= 10;
        }
    }

    // Safety: buf[idx..] はASCII数字のみで構成される
    unsafe { core::str::from_utf8_unchecked(&buf[idx..]) }
}

// ============================================================
// display_task: 全フレーム一括 DMA 転送
// ============================================================

#[embassy_executor::task]
async fn display_task(
    res: DisplayPeripherals,
    mut front: &'static mut FrameBuffer,
) {
    // DMA CH0/CH1 の所有権を保持（embassy による二重使用を防止）
    // 両チャネルとも PAC 直接操作 + MULTI_CHAN_TRIGGER で同時起動するため
    // embassy API では使用しない
    let _dma_ch0 = res.dma_ch0;
    let _dma_ch1 = res.dma_ch1;

    // === SM0: ピクセル出力 + NCLK (sideset) ===
    // 2命令反転版: side 1 でデータセットアップ、side 0 の立ち下がりでLCDサンプル
    let prg_pixel = pio_asm!(
        ".side_set 1",
        ".wrap_target",
        "    out pins, 18  side 1",   // データ出力 + NCLK HIGH（セットアップ期間）
        "    nop           side 0",   // NCLK LOW（立ち下がりでLCDサンプル）
        ".wrap",
    );

    // === SM1: HSYNC/VSYNC タイミング ===
    let prg_timing = pio_asm!(
        ".wrap_target",
        // VSYNC active line (1 line)
        "    set pins, 0",          // HSYNC=0, VSYNC=0
        "    pull block",
        "    mov x, osr",
        "hsync_v0:",
        "    jmp x-- hsync_v0",
        //
        "    set pins, 1",          // HSYNC=1, VSYNC=0
        "    pull block",
        "    mov x, osr",
        "rest_v0:",
        "    jmp x-- rest_v0",
        //
        // 通常ラインカウントロード
        "    pull block",
        "    mov y, osr",
        //
        // 通常ラインループ
        "normal_line:",
        "    set pins, 2",          // HSYNC=0, VSYNC=1
        "    pull block",
        "    mov x, osr",
        "hsync_v1:",
        "    jmp x-- hsync_v1",
        //
        "    set pins, 3",          // HSYNC=1, VSYNC=1
        "    pull block",
        "    mov x, osr",
        "rest_v1:",
        "    jmp x-- rest_v1",
        //
        "    jmp y-- normal_line",
        ".wrap",
    );

    let Pio {
        mut common,
        mut sm0,
        mut sm1,
        ..
    } = Pio::new(res.pio0, Irqs);

    // --- ピン設定 ---
    let pin2 = common.make_pio_pin(res.pin2);
    let pin3 = common.make_pio_pin(res.pin3);
    let pin4 = common.make_pio_pin(res.pin4);
    let pin5 = common.make_pio_pin(res.pin5);
    let pin6 = common.make_pio_pin(res.pin6);
    let pin7 = common.make_pio_pin(res.pin7);
    let pin8 = common.make_pio_pin(res.pin8);
    let pin9 = common.make_pio_pin(res.pin9);
    let pin10 = common.make_pio_pin(res.pin10);
    let pin11 = common.make_pio_pin(res.pin11);
    let pin12 = common.make_pio_pin(res.pin12);
    let pin13 = common.make_pio_pin(res.pin13);
    let pin14 = common.make_pio_pin(res.pin14);
    let pin15 = common.make_pio_pin(res.pin15);
    let pin16 = common.make_pio_pin(res.pin16);
    let pin17 = common.make_pio_pin(res.pin17);
    let pin18 = common.make_pio_pin(res.pin18);
    let pin19 = common.make_pio_pin(res.pin19);
    let nclk_pin = common.make_pio_pin(res.pin20);
    let hsync_pin = common.make_pio_pin(res.pin21);
    let vsync_pin = common.make_pio_pin(res.pin22);

    // ピン方向を出力に設定
    sm0.set_pin_dirs(Direction::Out, &[
        &pin2, &pin3, &pin4, &pin5, &pin6, &pin7, &pin8, &pin9, &pin10, &pin11, &pin12, &pin13,
        &pin14, &pin15, &pin16, &pin17, &pin18, &pin19, &nclk_pin,
    ]);
    sm1.set_pin_dirs(Direction::Out, &[&hsync_pin, &vsync_pin]);

    // --- SM0 設定 ---
    let loaded_pixel = common.load_program(&prg_pixel.program);
    let mut cfg0 = Config::default();
    cfg0.use_program(&loaded_pixel, &[&nclk_pin]);
    cfg0.set_out_pins(&[
        &pin2, &pin3, &pin4, &pin5, &pin6, &pin7, &pin8, &pin9, &pin10, &pin11, &pin12, &pin13,
        &pin14, &pin15, &pin16, &pin17, &pin18, &pin19,
    ]);
    cfg0.shift_out = ShiftConfig {
        auto_fill: true,
        threshold: 18,
        direction: ShiftDirection::Right,
    };
    cfg0.clock_divider = FixedU32::<U8>::from_bits(
        (PIO_CLK_DIV_INT as u32) << 8 | PIO_CLK_DIV_FRAC as u32,
    );
    cfg0.fifo_join = FifoJoin::TxOnly;

    // --- SM1 設定 ---
    let loaded_timing = common.load_program(&prg_timing.program);
    let mut cfg1 = Config::default();
    cfg1.use_program(&loaded_timing, &[]);
    cfg1.set_set_pins(&[&hsync_pin, &vsync_pin]);
    cfg1.clock_divider = FixedU32::<U8>::from_bits(SM1_CLK_DIV_BITS);
    cfg1.fifo_join = FifoJoin::TxOnly;

    sm0.set_config(&cfg0);
    sm1.set_config(&cfg1);

    // 両 SM を同時に開始
    common.apply_sm_batch(|batch| {
        batch.set_enable(&mut sm0, true);
        batch.set_enable(&mut sm1, true);
    });

    // DMA 書き込み先アドレス (PIO0 TX FIFO) — ループ中不変
    let sm0_txf_addr = embassy_rp::pac::PIO0.txf(0).as_ptr() as u32;
    let sm1_txf_addr = embassy_rp::pac::PIO0.txf(1).as_ptr() as u32;

    // === 初回 DMA 設定: WRITE_ADDR と CTRL は固定のため一度だけ設定 ===
    {
        let dma = embassy_rp::pac::DMA;

        // --- CH0 (SM0: ピクセルデータ) WRITE_ADDR + CTRL ---
        let ch0 = dma.ch(0);
        ch0.write_addr().write_value(sm0_txf_addr);
        // al1_ctrl: 非トリガーエイリアス — 書き込んでもDMA起動しない
        {
            let mut ctrl = embassy_rp::pac::dma::regs::CtrlTrig(0);
            ctrl.set_en(true);
            ctrl.set_data_size(embassy_rp::pac::dma::vals::DataSize::SIZE_WORD);
            ctrl.set_incr_read(true);
            ctrl.set_incr_write(false);
            ctrl.set_treq_sel(embassy_rp::pac::dma::vals::TreqSel::PIO0_TX0);
            ctrl.set_chain_to(0); // 自CH = チェイン無効化
            ch0.al1_ctrl().write_value(ctrl.0);
        }

        // --- CH1 (SM1: HSYNC/VSYNC タイミング) WRITE_ADDR + CTRL ---
        let ch1 = dma.ch(1);
        ch1.write_addr().write_value(sm1_txf_addr);
        {
            let mut ctrl = embassy_rp::pac::dma::regs::CtrlTrig(0);
            ctrl.set_en(true);
            ctrl.set_data_size(embassy_rp::pac::dma::vals::DataSize::SIZE_WORD);
            ctrl.set_incr_read(true);
            ctrl.set_incr_write(false);
            ctrl.set_treq_sel(embassy_rp::pac::dma::vals::TreqSel::PIO0_TX1);
            ctrl.set_chain_to(1); // 自CH = チェイン無効化
            ch1.al1_ctrl().write_value(ctrl.0);
        }
    }

    // === フレームループ: 全フレーム一括 DMA 転送 ===
    //
    // 方式: MULTI_CHAN_TRIGGER による完全同時起動
    //   - 初回含め毎フレーム READ_ADDR + TRANS_COUNT を再設定
    //   - DMA設定〜TRIGGERをクリティカルセクションで保護し、
    //     割り込みによるFIFOデータ途切れを防止
    //   - FIFO drain待ちは行わない（次DMAが即座にデータ供給し途切れなし）
    // 初回フレーム起動（クリティカルセクション内）
    cortex_m::interrupt::free(|_| {
        let dma = embassy_rp::pac::DMA;

        let ch0 = dma.ch(0);
        ch0.read_addr()
            .write_value(front.frame_data().as_ptr() as u32);
        ch0.trans_count().write(|w| {
            w.set_count(front.frame_data().len() as u32);
        });

        let ch1 = dma.ch(1);
        ch1.read_addr()
            .write_value(SM1_FRAME_DATA.as_ptr() as u32);
        ch1.trans_count().write(|w| {
            w.set_count(SM1_FRAME_SIZE as u32);
        });

        dma.multi_chan_trigger().write(|w| {
            w.set_multi_chan_trigger(0b11);
        });
    });

    loop {
        // --- CH0 完了待ち (ポーリング + yield) ---
        // CH0 (SM0, 57344ワード) は CH1 (SM1, 225ワード) より後に完了する。
        // CH0 の BUSY=false を待てば両チャネルとも完了済み。
        let dma = embassy_rp::pac::DMA;
        loop {
            if !dma.ch(0).ctrl_trig().read().busy() {
                break;
            }
            embassy_futures::yield_now().await;
        }

        let boundary_start = Instant::now();

        // VSYNC 境界: swap チェック
        if let Ok(new_front) = SWAP_CH.try_receive() {
            RETURN_CH.send(front).await;
            front = new_front;
        }

        // クリティカルセクション: DMA再設定〜TRIGGERを割り込み無しで実行
        // FIFO残データが消費される前に次DMAを開始し、データ途切れを防ぐ
        // WRITE_ADDR, CTRL は固定のため再設定不要（初回のみ設定済み）
        cortex_m::interrupt::free(|_| {
            let dma = embassy_rp::pac::DMA;

            // CH0: READ_ADDR + TRANS_COUNT のみ再設定
            let ch0 = dma.ch(0);
            ch0.read_addr()
                .write_value(front.frame_data().as_ptr() as u32);
            ch0.trans_count().write(|w| {
                w.set_count(front.frame_data().len() as u32);
            });

            // CH1: READ_ADDR + TRANS_COUNT のみ再設定
            let ch1 = dma.ch(1);
            ch1.read_addr()
                .write_value(SM1_FRAME_DATA.as_ptr() as u32);
            ch1.trans_count().write(|w| {
                w.set_count(SM1_FRAME_SIZE as u32);
            });

            // 両チャネル同時起動
            dma.multi_chan_trigger().write(|w| {
                w.set_multi_chan_trigger(0b11);
            });
        });

        let boundary_us = saturating_u64_to_u32(boundary_start.elapsed().as_micros());
        DISPLAY_BOUNDARY_SECTION_US_LATEST.store(boundary_us, Ordering::Relaxed);
        update_max_atomic(&DISPLAY_BOUNDARY_SECTION_US_MAX, boundary_us);
    }
}

// ============================================================
// main: embedded-graphics 描画タスク
// ============================================================

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    // フレームバッファ初期化 (BSS 領域から &'static mut を取得)
    // Safety: 各バッファは初期化後、一方が display_task に、他方が main に
    // 排他的に渡される。以降は Channel を通じて所有権が移動するため安全。
    let fb_a: &'static mut FrameBuffer = unsafe { &mut *addr_of_mut!(FB_A_DATA) };
    let fb_b: &'static mut FrameBuffer = unsafe { &mut *addr_of_mut!(FB_B_DATA) };

    // display_task 起動 (fb_a を初期 front として渡す)
    spawner
        .spawn(display_task(
            DisplayPeripherals {
                pio0: p.PIO0,
                pin2: p.PIN_2,
                pin3: p.PIN_3,
                pin4: p.PIN_4,
                pin5: p.PIN_5,
                pin6: p.PIN_6,
                pin7: p.PIN_7,
                pin8: p.PIN_8,
                pin9: p.PIN_9,
                pin10: p.PIN_10,
                pin11: p.PIN_11,
                pin12: p.PIN_12,
                pin13: p.PIN_13,
                pin14: p.PIN_14,
                pin15: p.PIN_15,
                pin16: p.PIN_16,
                pin17: p.PIN_17,
                pin18: p.PIN_18,
                pin19: p.PIN_19,
                pin20: p.PIN_20,
                pin21: p.PIN_21,
                pin22: p.PIN_22,
                dma_ch0: p.DMA_CH0,
                dma_ch1: p.DMA_CH1,
            },
            fb_a,
        ))
        .unwrap();

    // メインタスク: embedded-graphics で描画
    let mut back: &'static mut FrameBuffer = fb_b;
    let mut hud_div: u32 = 0;

    let text_style = MonoTextStyle::new(&FONT_6X10, Rgb666::WHITE);

    let mut draw_cpu_latest_buf = [0u8; 10];
    let mut draw_cpu_max_buf = [0u8; 10];
    let mut frame_loop_latest_buf = [0u8; 10];
    let mut frame_loop_max_buf = [0u8; 10];

    let mut draw_cpu_latest_text: &str = "0";
    let mut draw_cpu_max_text: &str = "0";
    let mut frame_loop_latest_text: &str = "0";
    let mut frame_loop_max_text: &str = "0";

    loop {
        let frame_loop_start = Instant::now();

        if hud_div == 0 {
            let draw_cpu_latest = DRAW_CPU_MS_LATEST.load(Ordering::Relaxed);
            let draw_cpu_max = DRAW_CPU_MS_MAX.load(Ordering::Relaxed);
            let frame_loop_latest = FRAME_LOOP_MS_LATEST.load(Ordering::Relaxed);
            let frame_loop_max = FRAME_LOOP_MS_MAX.load(Ordering::Relaxed);

            draw_cpu_latest_text = write_u32_decimal(draw_cpu_latest, &mut draw_cpu_latest_buf);
            draw_cpu_max_text = write_u32_decimal(draw_cpu_max, &mut draw_cpu_max_buf);
            frame_loop_latest_text = write_u32_decimal(frame_loop_latest, &mut frame_loop_latest_buf);
            frame_loop_max_text = write_u32_decimal(frame_loop_max, &mut frame_loop_max_buf);
        }

        let draw_cpu_start = Instant::now();

        DrawTarget::clear(back, Rgb666::BLACK).unwrap();

        // テキスト描画
        Text::new(
            "Hello, 300yen LCD!",
            embedded_graphics::geometry::Point::new(10, 12),
            text_style,
        )
        .draw(back)
        .unwrap();

        Text::new(
            "Full-frame DMA",
            embedded_graphics::geometry::Point::new(10, 26),
            text_style,
        )
        .draw(back)
        .unwrap();

        // 赤い矩形
        Rectangle::new(
            embedded_graphics::geometry::Point::new(10, 35),
            Size::new(60, 30),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb666::RED))
        .draw(back)
        .unwrap();

        // 緑の円
        Circle::new(embedded_graphics::geometry::Point::new(90, 35), 30)
            .into_styled(PrimitiveStyle::with_fill(Rgb666::GREEN))
            .draw(back)
            .unwrap();

        // 青い矩形
        Rectangle::new(
            embedded_graphics::geometry::Point::new(140, 35),
            Size::new(60, 30),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb666::BLUE))
        .draw(back)
        .unwrap();

        // 白い線
        Line::new(
            embedded_graphics::geometry::Point::new(10, 75),
            embedded_graphics::geometry::Point::new(390, 75),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb666::WHITE, 1))
        .draw(back)
        .unwrap();

        // カラーバー
        let colors = [
            Rgb666::WHITE,
            Rgb666::new(63, 63, 0),
            Rgb666::new(0, 63, 63),
            Rgb666::GREEN,
            Rgb666::new(63, 0, 63),
            Rgb666::RED,
            Rgb666::BLUE,
            Rgb666::BLACK,
        ];
        for (i, &color) in colors.iter().enumerate() {
            let x = i as i32 * 50;
            Rectangle::new(
                embedded_graphics::geometry::Point::new(x, 80),
                Size::new(50, 16),
            )
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(back)
            .unwrap();
        }

        // 計測 HUD (右上固定、表示内容は間引き更新)
        Text::new(
            "CPUms L:",
            embedded_graphics::geometry::Point::new(HUD_X, HUD_LINE1_Y),
            text_style,
        )
        .draw(back)
        .unwrap();
        Text::new(
            draw_cpu_latest_text,
            embedded_graphics::geometry::Point::new(HUD_X + 45, HUD_LINE1_Y),
            text_style,
        )
        .draw(back)
        .unwrap();
        Text::new(
            " M:",
            embedded_graphics::geometry::Point::new(HUD_X + 75, HUD_LINE1_Y),
            text_style,
        )
        .draw(back)
        .unwrap();
        Text::new(
            draw_cpu_max_text,
            embedded_graphics::geometry::Point::new(HUD_X + 93, HUD_LINE1_Y),
            text_style,
        )
        .draw(back)
        .unwrap();

        Text::new(
            "FRMms L:",
            embedded_graphics::geometry::Point::new(HUD_X, HUD_LINE2_Y),
            text_style,
        )
        .draw(back)
        .unwrap();
        Text::new(
            frame_loop_latest_text,
            embedded_graphics::geometry::Point::new(HUD_X + 45, HUD_LINE2_Y),
            text_style,
        )
        .draw(back)
        .unwrap();
        Text::new(
            " M:",
            embedded_graphics::geometry::Point::new(HUD_X + 75, HUD_LINE2_Y),
            text_style,
        )
        .draw(back)
        .unwrap();
        Text::new(
            frame_loop_max_text,
            embedded_graphics::geometry::Point::new(HUD_X + 93, HUD_LINE2_Y),
            text_style,
        )
        .draw(back)
        .unwrap();

        let draw_cpu_ms = saturating_u64_to_u32(draw_cpu_start.elapsed().as_millis());
        DRAW_CPU_MS_LATEST.store(draw_cpu_ms, Ordering::Relaxed);
        update_max_atomic(&DRAW_CPU_MS_MAX, draw_cpu_ms);

        SWAP_CH.send(back).await;
        back = RETURN_CH.receive().await;

        let frame_loop_ms = saturating_u64_to_u32(frame_loop_start.elapsed().as_millis());
        FRAME_LOOP_MS_LATEST.store(frame_loop_ms, Ordering::Relaxed);
        update_max_atomic(&FRAME_LOOP_MS_MAX, frame_loop_ms);

        hud_div = (hud_div + 1) % HUD_UPDATE_INTERVAL_FRAMES;
    }
}
