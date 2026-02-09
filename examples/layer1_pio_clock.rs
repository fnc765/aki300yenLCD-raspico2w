//! Layer 1: PIO NCLK クロック生成テスト
//!
//! PIO を使って GP20 に NCLK クロック信号を生成する。
//! 目標周波数: 約 3.44 MHz (60fps時のピクセルクロック)
//!
//! # 合格基準
//! - [ ] GP20 に約 3.4MHz のクロック信号が確認できる
//! - [ ] デューティ比が約50%
//! - [ ] ジッターが十分に小さい

#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{Config, InterruptHandler, Pio};
use embassy_rp::pio::program::pio_asm;
use embassy_rp::bind_interrupts;
use fixed::traits::ToFixed;
use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("Layer 1: PIO NCLK clock generation test");

    // PIO プログラム: NCLK クロック生成
    // sideset 1 ピンで GP20 を制御
    let prg = pio_asm!(
        ".side_set 1",
        ".wrap_target",
        "    nop side 1",    // NCLK = HIGH
        "    nop side 0",    // NCLK = LOW
        ".wrap",
    );

    let Pio {
        mut common,
        sm0: mut sm,
        ..
    } = Pio::new(p.PIO0, Irqs);

    // NCLK ピン (GP20) を PIO に割り当て
    let nclk_pin = common.make_pio_pin(p.PIN_20);

    let mut cfg = Config::default();
    cfg.use_program(&common.load_program(&prg.program), &[&nclk_pin]);

    // クロック分周: 150MHz / (3.44MHz × 2) ≈ 21.8
    // 2 PIO サイクルで 1 NCLK 周期 (HIGH + LOW)
    // 分周比 21.8 → 実際の NCLK = 150MHz / (21.8 × 2) ≈ 3.44 MHz
    cfg.clock_divider = (21u32 << 8 | 204u32).to_fixed();

    sm.set_config(&cfg);
    sm.set_enable(true);

    let actual_freq = 150_000_000u32 / (21 * 2 + 1); // 概算
    info!("NCLK started on GP20");
    info!("Target: 3.44 MHz, Approx: {} Hz", actual_freq);
    info!("Measure with oscilloscope to verify frequency and duty cycle");

    loop {
        embassy_time::Timer::after_secs(10).await;
        info!("NCLK running...");
    }
}
