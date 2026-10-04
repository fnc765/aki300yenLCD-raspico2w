#![no_std]
#![no_main]

mod controller;
mod network;
mod usb;
mod config {
    include!(concat!(env!("OUT_DIR"), "/probe_config.rs"));
}

use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, Ordering};
use cyw43::{JoinAuth, JoinOptions, aligned_bytes};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_net::{ConfigV6, Ipv6Cidr, StackResources, StaticConfigV6};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::executor::InterruptExecutor;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::interrupt;
use embassy_rp::interrupt::{InterruptExt, Priority};
use embassy_rp::peripherals::{DMA_CH0, PIO0, USB};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::usb::{Driver, InterruptHandler as UsbInterruptHandler};
use embassy_time::{Duration, Timer, with_timeout};
use static_cell::StaticCell;

bind_interrupts!(struct Irqs {
    USBCTRL_IRQ => UsbInterruptHandler<USB>;
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    DMA_IRQ_0 => embassy_rp::dma::InterruptHandler<DMA_CH0>;
});

#[global_allocator]
static HEAP: embedded_alloc::LlffHeap = embedded_alloc::LlffHeap::empty();
static STAGE: AtomicU32 = AtomicU32::new(0);
static USB_EXECUTOR: InterruptExecutor = InterruptExecutor::new();

#[interrupt]
unsafe fn SWI_IRQ_0() {
    // SAFETY: this interrupt is reserved for the initialized USB executor.
    unsafe { USB_EXECUTOR.on_interrupt() }
}

// Construct non-Send USB state on its own executor. USB control requests and
// logging can then run while Matter crypto or a failed Wi-Fi task occupies main.
#[embassy_executor::task]
async fn start_usb(driver: Driver<'static, USB>) {
    // SAFETY: this function runs as a task on USB_EXECUTOR.
    let spawner = unsafe { Spawner::for_current_executor().await };
    spawner.spawn(usb::logger_task(driver).unwrap());
    spawner.spawn(heartbeat().unwrap());
}

#[embassy_executor::task]
async fn heartbeat() {
    loop {
        Timer::after_secs(5).await;
        log::info!("PROBE_HEARTBEAT stage={}", STAGE.load(Ordering::Relaxed));
    }
}

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, cyw43::SpiBus<Output<'static>, PioSpi<'static, PIO0, 0>>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::main(
    executor = "embassy_rp::executor::Executor",
    entry = "cortex_m_rt::entry"
)]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    static HEAP_SPACE: StaticCell<MaybeUninit<[u8; 32768]>> = StaticCell::new();
    let heap = HEAP_SPACE.init(MaybeUninit::uninit());
    // SAFETY: only the allocator owns this static storage after initialization.
    unsafe {
        HEAP.init(heap.as_mut_ptr() as usize, 32768);
    }
    interrupt::SWI_IRQ_0.set_priority(Priority::P3);
    USB_EXECUTOR
        .start(interrupt::SWI_IRQ_0)
        .spawn(start_usb(Driver::new(p.USB, Irqs)).unwrap());
    Timer::after_secs(4).await;
    log::info!("PROBE_BOOT board=RP2350 mode=standalone-matter lcd=unused");
    for _ in 0..60 {
        if usb::START.load(Ordering::Relaxed) {
            break;
        }
        Timer::after_secs(1).await;
    }
    if !config::CONFIGURED {
        idle("SETTINGS_REQUIRED: fill probe.local.toml and rebuild").await;
    }

    let fw = aligned_bytes!("../../.local/matter-probe/embassy/cyw43-firmware/43439A0.bin");
    let clm = aligned_bytes!("../../.local/matter-probe/embassy/cyw43-firmware/43439A0_clm.bin");
    let nvram = aligned_bytes!("../../.local/matter-probe/embassy/cyw43-firmware/nvram_rp2040.bin");
    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        RM2_CLOCK_DIVIDER,
        pio.irq0,
        cs,
        p.PIN_24,
        p.PIN_29,
        embassy_rp::dma::Channel::new(p.DMA_CH0, Irqs),
    );
    static WIFI_STATE: StaticCell<cyw43::State> = StaticCell::new();
    log::info!("WIFI_INIT starting CYW43439");
    STAGE.store(1, Ordering::Relaxed);
    let (device, mut control, runner) =
        cyw43::new(WIFI_STATE.init(cyw43::State::new()), pwr, spi, fw, nvram).await;
    spawner.spawn(wifi_task(runner).unwrap());
    log::info!("WIFI_DRIVER_READY");
    STAGE.store(2, Ordering::Relaxed);
    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::None)
        .await;
    let mac = control.address().await;
    let ipv6 = core::net::Ipv6Addr::from([
        0xfe,
        0x80,
        0,
        0,
        0,
        0,
        0,
        0,
        mac[0] ^ 2,
        mac[1],
        mac[2],
        0xff,
        0xfe,
        mac[3],
        mac[4],
        mac[5],
    ]);
    for address in [
        [0x01, 0x00, 0x5e, 0x00, 0x00, 0xfb],       // IPv4 mDNS
        [0x33, 0x33, 0x00, 0x00, 0x00, 0xfb],       // IPv6 mDNS
        [0x33, 0x33, 0x00, 0x00, 0x00, 0x01],       // IPv6 all nodes
        [0x33, 0x33, 0xff, mac[3], mac[4], mac[5]], // Neighbor discovery
    ] {
        control.add_multicast_address(address).await.unwrap();
    }
    let mut network_config = embassy_net::Config::dhcpv4(Default::default());
    network_config.ipv6 = ConfigV6::Static(StaticConfigV6 {
        address: Ipv6Cidr::new(ipv6, 64),
        gateway: None,
        dns_servers: heapless::Vec::new(),
    });
    static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
    let mut rng = RoscRng;
    let (stack, runner) = embassy_net::new(
        device,
        network_config,
        RESOURCES.init(StackResources::new()),
        rng.next_u64(),
    );
    spawner.spawn(net_task(runner).unwrap());
    let mut join_options = JoinOptions::new(config::WIFI_PASSWORD.as_bytes());
    join_options.auth = JoinAuth::Wpa2;
    STAGE.store(3, Ordering::Relaxed);
    for attempt in 1..=3 {
        log::info!("WIFI_JOIN attempt={attempt} auth=WPA2 (credentials redacted)");
        match with_timeout(
            Duration::from_secs(30),
            control.join(config::WIFI_SSID, join_options.clone()),
        )
        .await
        {
            Ok(Ok(())) => {
                log::info!("WIFI_JOIN_OK");
                break;
            }
            Ok(Err(e)) => log::warn!("WIFI_JOIN_FAILED: {e:?}"),
            Err(_) => log::warn!("WIFI_JOIN_TIMEOUT"),
        }
        if attempt == 3 {
            idle("WIFI_FAILED: retry limit reached").await;
        }
        if with_timeout(Duration::from_secs(5), control.leave())
            .await
            .is_err()
        {
            idle("WIFI_LEAVE_TIMEOUT").await;
        }
        Timer::after_secs(5).await;
    }
    if with_timeout(Duration::from_secs(30), async {
        while stack.config_v4().is_none() {
            Timer::after_millis(100).await;
        }
    })
    .await
    .is_err()
    {
        idle("DHCP_TIMEOUT").await;
    }
    let ipv4: core::net::Ipv4Addr = stack.config_v4().unwrap().address.address();
    log::info!("NETWORK_READY ipv4={ipv4} ipv6={ipv6}");
    STAGE.store(4, Ordering::Relaxed);
    control.gpio_set(0, true).await;
    if let Err(e) = controller::run(stack, ipv4, ipv6, &mut rng).await {
        log::error!("MATTER_PROBE_FAILED: {e:?}");
        idle("MATTER_STOPPED: no automatic commissioning retry").await;
    }
    idle("MATTER_PROBE_FINISHED").await;
}

async fn idle(message: &'static str) -> ! {
    loop {
        log::info!("{message}");
        Timer::after_secs(10).await;
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
    STAGE.store(99, Ordering::Relaxed);
    log::error!("PANIC: {info}");
    loop {
        cortex_m::asm::wfi();
    }
}
