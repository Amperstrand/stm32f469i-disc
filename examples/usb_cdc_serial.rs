//! HIL test: USB OTG FS peripheral initialization.
//!
//! Proves USB peripheral init succeeds (PLL48CLK, USB device config).
//! Does NOT test host-side serial communication (requires USB cable + host).
//! The USB device is created and initialized; if no panic occurs, the
//! hardware setup is correct.
//!
//! Run: `cargo run --release --example usb_cdc_serial --features usb_fs`

#![no_std]
#![no_main]

use panic_probe as _;

use cortex_m_rt::entry;
use defmt_rtt as _;
use static_cell::ConstStaticCell;
use stm32f469i_disc::{hal, hal::pac, hal::prelude::*, usb};

use hal::otg_fs::UsbBus;
use usb_device::prelude::*;

static EP_MEMORY: ConstStaticCell<[u32; 1024]> = ConstStaticCell::new([0; 1024]);

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();

    let mut rcc = dp.RCC.freeze(
        hal::rcc::Config::hse(8.MHz())
            .sysclk(168.MHz())
            .require_pll48clk(),
    );

    defmt::info!("USB CDC Serial HIL test starting");

    let gpioa = dp.GPIOA.split(&mut rcc);

    let usb = usb::init(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        gpioa.pa11,
        gpioa.pa12,
        &rcc.clocks,
    );

    defmt::info!("USB peripheral initialized");

    let usb_bus = UsbBus::new(usb, EP_MEMORY.take());

    let _serial = usbd_serial::SerialPort::new(&usb_bus);

    let _usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .device_class(usbd_serial::USB_CLASS_CDC)
        .strings(&[StringDescriptors::default()
            .manufacturer("STM32F469")
            .product("CDC Serial")
            .serial_number("DISCO1")])
        .unwrap()
        .build();

    defmt::info!("USB device created successfully");
    defmt::info!("HIL_RESULT:usb_cdc_serial:PASS");

    loop {
        cortex_m::asm::wfi();
    }
}
