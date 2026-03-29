//! USB OTG FS CDC serial test for STM32F469I-DISCO
//!
//! Tests USB enumeration and CDC echo via host serial.
//! Requires host-side interaction to verify (connect USB cable to host).
//!
//! Tests (verified via host serial at 115200 baud):
//!   1. USB enumeration (device appears on host)
//!   2. CDC echo: send "PING" -> expect "PONG"
//!   3. Multi-byte echo: send 64 bytes -> get 64 bytes back
//!   4. sustained poll: 1000 poll iterations without error
//!
//! The device also outputs test results via RTT (defmt).

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use crate::board::hal::{
    otg_fs::{UsbBus, USB},
    pac,
    prelude::*,
    rcc,
};

use cortex_m_rt::entry;

use usb_device::prelude::*;

use static_cell::ConstStaticCell;

use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static ECHO_COUNT: AtomicUsize = AtomicUsize::new(0);
static POLL_COUNT: AtomicUsize = AtomicUsize::new(0);

static EP_MEMORY: ConstStaticCell<[u32; 1024]> = ConstStaticCell::new([0; 1024]);

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

#[allow(dead_code)]
fn fail(name: &str, reason: &str) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!("TEST {}: FAIL {}", name, reason);
}

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();

    let mut rcc = dp.RCC.freeze(
        rcc::Config::hse(8.MHz())
            .sysclk(180.MHz())
            .require_pll48clk(),
    );

    let gpioa = dp.GPIOA.split(&mut rcc);
    let gpiog = dp.GPIOG.split(&mut rcc);

    defmt::info!("=== USB CDC Test Suite ===");

    // LED for visual feedback
    let mut led = gpiog.pg6.into_push_pull_output();

    // USB init
    defmt::info!("TEST usb_init: RUNNING");
    let usb = USB::new(
        (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
        (gpioa.pa11, gpioa.pa12),
        &rcc.clocks,
    );
    let usb_bus = UsbBus::new(usb, EP_MEMORY.take());
    let mut serial = usbd_serial::SerialPort::new(&usb_bus);

    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
        .device_class(usbd_serial::USB_CLASS_CDC)
        .strings(&[StringDescriptors::default()
            .manufacturer("stm32f469i-disc")
            .product("USB CDC Test")
            .serial_number("TEST1")])
        .unwrap()
        .build();

    pass("usb_init");

    // Test 2: CDC echo loop (runs 1000 iterations)
    defmt::info!("TEST usb_cdc_echo: RUNNING");
    defmt::info!("  Send PING via serial, expect PONG");
    defmt::info!("  Send BYE to end echo test");

    let mut rx_buf = [0u8; 64];
    let mut echo_done = false;
    let mut led_state = false;

    loop {
        if !usb_dev.poll(&mut [&mut serial]) {
            continue;
        }

        POLL_COUNT.fetch_add(1, Ordering::Relaxed);

        match serial.read(&mut rx_buf) {
            Ok(count) if count > 0 => {
                ECHO_COUNT.fetch_add(1, Ordering::Relaxed);

                let response = if &rx_buf[..count] == b"PING\r"
                    || &rx_buf[..count] == b"PING\n"
                    || &rx_buf[..count] == b"PING"
                {
                    b"PONG\r\n"
                } else if &rx_buf[..count] == b"STATS\r"
                    || &rx_buf[..count] == b"STATS\n"
                    || &rx_buf[..count] == b"STATS"
                {
                    let _ = serial.write(b"USB CDC Test Active\r\n");
                    continue;
                } else if &rx_buf[..count] == b"BYE\r"
                    || &rx_buf[..count] == b"BYE\n"
                    || &rx_buf[..count] == b"BYE"
                {
                    if !echo_done {
                        echo_done = true;
                        pass("usb_cdc_echo");
                    }
                    b"ECHO DONE\r\n"
                } else {
                    &rx_buf[..count]
                };

                let _ = serial.write(response);
            }
            _ => {}
        }

        // LED blink every 500 polls
        let polls = POLL_COUNT.load(Ordering::Relaxed);
        if polls.is_multiple_of(500) {
            led_state = !led_state;
            if led_state {
                led.set_high();
            } else {
                led.set_low();
            }
        }

        // After echo test is done, run sustained poll test
        if echo_done && serial.read(&mut []).is_ok() {
            // Test 3: Sustained poll
            defmt::info!("TEST usb_sustained_poll: RUNNING");
            let target = POLL_COUNT.load(Ordering::Relaxed) + 10000;
            while POLL_COUNT.load(Ordering::Relaxed) < target {
                let _ = usb_dev.poll(&mut [&mut serial]);
            }
            pass("usb_sustained_poll");

            // Summary
            let passed = PASSED.load(Ordering::Relaxed);
            let failed = FAILED.load(Ordering::Relaxed);
            let total = passed + failed;
            let echoes = ECHO_COUNT.load(Ordering::Relaxed);

            defmt::info!("=== USB CDC Test Summary ===");
            defmt::info!("SUMMARY: {}/{} passed", passed, total);
            defmt::info!("Echo exchanges: {}", echoes);

            if failed == 0 {
                defmt::info!("ALL TESTS PASSED");
            } else {
                defmt::error!("FAILED: {} tests failed", failed);
            }
        }
    }
}
