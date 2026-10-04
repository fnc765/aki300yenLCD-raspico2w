use core::sync::atomic::{AtomicBool, Ordering};
use embassy_futures::join::join;
use embassy_rp::{peripherals::USB, usb::Driver};
use embassy_usb::class::cdc_acm::{CdcAcmClass, State};
use embassy_usb::control::{OutResponse, Recipient, Request, RequestType};
use embassy_usb::msos::{
    CompatibleIdFeatureDescriptor, PropertyData, RegistryPropertyFeatureDescriptor, windows_version,
};
use embassy_usb::types::{InterfaceNumber, StringIndex};
use embassy_usb::{Builder, Config, Handler};
use static_cell::StaticCell;

pub static START: AtomicBool = AtomicBool::new(false);
struct StartHandler;
impl embassy_usb_logger::ReceiverHandler for StartHandler {
    fn new() -> Self {
        Self
    }
    async fn handle_data(&self, data: &[u8]) {
        if data.starts_with(b"START") {
            START.store(true, Ordering::Relaxed);
        }
    }
}

// RP2 reset request on its own WinUSB interface, alongside USB serial logs.
struct ResetHandler {
    interface: InterfaceNumber,
    description: StringIndex,
}
impl Handler for ResetHandler {
    fn get_string(&mut self, index: StringIndex, _: u16) -> Option<&str> {
        (index == self.description).then_some("rp2xxx-reset")
    }
    fn control_out(&mut self, r: Request, _: &[u8]) -> Option<OutResponse> {
        if r.request_type == RequestType::Class
            && r.recipient == Recipient::Interface
            && r.index == u8::from(self.interface) as u16
            && r.request == 1
        {
            embassy_rp::rom_data::reset_to_usb_boot(0, 0);
            Some(OutResponse::Accepted)
        } else {
            None
        }
    }
}

fn chip_serial() -> Option<&'static str> {
    let mut info = [0u32; 9];
    // SAFETY: buffer size and requested ROM CHIP_INFO mask match the SDK API.
    let words = unsafe { embassy_rp::rom_data::get_sys_info(info.as_mut_ptr(), info.len(), 1) };
    if words != 4 || info[0] != 1 {
        return None;
    }
    let id = (u64::from(info[3]) << 32) | u64::from(info[2]);
    static SERIAL: StaticCell<[u8; 16]> = StaticCell::new();
    let serial = SERIAL.init([0; 16]);
    for (i, c) in serial.iter_mut().enumerate() {
        *c = b"0123456789ABCDEF"[((id >> (60 - i * 4)) & 15) as usize];
    }
    core::str::from_utf8(serial).ok()
}

#[embassy_executor::task]
pub async fn logger_task(driver: Driver<'static, USB>) {
    static CONFIG: StaticCell<[u8; 512]> = StaticCell::new();
    static BOS: StaticCell<[u8; 256]> = StaticCell::new();
    static MSOS: StaticCell<[u8; 256]> = StaticCell::new();
    static CONTROL: StaticCell<[u8; 64]> = StaticCell::new();
    static CDC_STATE: StaticCell<State> = StaticCell::new();
    static RESET: StaticCell<ResetHandler> = StaticCell::new();
    let mut config = Config::new(0x2e8a, 0x000a);
    config.manufacturer = Some("Raspberry Pi");
    config.product = Some("Pico 2 W Matter Probe");
    config.device_release = 0x0101;
    config.serial_number = chip_serial();
    config.max_packet_size_0 = 64;
    config.max_power = 100;
    let mut builder = Builder::new(
        driver,
        config,
        CONFIG.init([0; 512]),
        BOS.init([0; 256]),
        MSOS.init([0; 256]),
        CONTROL.init([0; 64]),
    );
    builder.msos_descriptor(windows_version::WIN8_1, 0x20);
    let logger_class = CdcAcmClass::new(&mut builder, CDC_STATE.init(State::new()), 64);
    let handler = {
        let mut function = builder.function(0xff, 0, 1);
        function.msos_feature(CompatibleIdFeatureDescriptor::new("WINUSB", ""));
        function.msos_feature(RegistryPropertyFeatureDescriptor::new(
            "DeviceInterfaceGUIDs",
            PropertyData::RegMultiSz(&["{CDB3B5AD-293B-4663-AA36-1AAE46463776}"]),
        ));
        let mut interface = function.interface();
        let number = interface.interface_number();
        let description = interface.string();
        interface.alt_setting(0xff, 0, 1, Some(description));
        ResetHandler {
            interface: number,
            description,
        }
    };
    builder.handler(RESET.init(handler));
    let mut device = builder.build();
    static LOGGER: StaticCell<embassy_usb_logger::UsbLogger<8192, StartHandler>> =
        StaticCell::new();
    let logger = LOGGER.init(embassy_usb_logger::UsbLogger::new());
    logger.with_handler(StartHandler);
    // SAFETY: this is the only logger initialization, before starting the application.
    unsafe {
        log::set_logger_racy(&*logger).unwrap();
        log::set_max_level_racy(log::LevelFilter::Info);
    }
    join(device.run(), logger.create_future_from_class(logger_class)).await;
}
