//! Standalone USB CDC test for STM32F469I-DISCO
//!
//! This firmware has NO RTT, NO defmt, NO panic_probe. It communicates
//! test results ONLY via USB CDC serial. This is the correct way to test
//! USB because probe-rs/RTT interferes with USB timing.
//!
//! Flash with: st-flash --connect-under-reset write firmware.bin 0x08000000
//! Then: st-flash --connect-under-reset reset
//! Then: disconnect ST-Link (or at minimum, don't attach probe-rs)
//! Then: python3 tests/host/test_usb_host.py
//!
//! Protocol:
//!   Send "PING"  -> Receive "PONG <count>"
//!   Send "STATS" -> Receive status line
//!   Send "BYE"   -> Receive summary + "BYE DONE"
//!   Send anything else -> echoed back

#![no_main]
#![no_std]

use panic_halt as _;

use stm32f469i_disc as board;

use board::hal::{
    otg_fs::{UsbBus, USB},
    pac,
    prelude::*,
    rcc,
};

use cortex_m_rt::entry;
use usb_device::prelude::*;

use static_cell::ConstStaticCell;

use core::sync::atomic::{AtomicUsize, Ordering};

static EP_MEMORY: ConstStaticCell<[u32; 1024]> = ConstStaticCell::new([0; 1024]);

static ECHO_COUNT: AtomicUsize = AtomicUsize::new(0);
static PASS_COUNT: AtomicUsize = AtomicUsize::new(0);
static FAIL_COUNT: AtomicUsize = AtomicUsize::new(0);

fn write_str(serial: &mut usbd_serial::SerialPort<UsbBus<USB>>, s: &[u8]) {
    let _ = serial.write(s);
}

fn write_ok(serial: &mut usbd_serial::SerialPort<UsbBus<USB>>) {
    PASS_COUNT.fetch_add(1, Ordering::Relaxed);
    write_str(serial, b"PASS\r\n");
}

#[allow(dead_code)]
fn write_fail(serial: &mut usbd_serial::SerialPort<UsbBus<USB>>, reason: &[u8]) {
    FAIL_COUNT.fetch_add(1, Ordering::Relaxed);
    write_str(serial, b"FAIL ");
    write_str(serial, reason);
    write_str(serial, b"\r\n");
}

#[allow(dead_code)]
#[allow(unused_assignments)]
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

    let mut led = gpiog.pg6.into_push_pull_output();

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
            .product("USB Standalone Test")
            .serial_number("USBSTAND1")])
        .unwrap()
        .build();

    let mut rx_buf = [0u8; 128];
    let mut phase: u8 = 0;
    let mut led_state = false;
    let mut poll_count: usize = 0;
    let mut stress_target: usize = 0;

    loop {
        if !usb_dev.poll(&mut [&mut serial]) {
            continue;
        }

        poll_count += 1;

        match serial.read(&mut rx_buf) {
            Ok(count) if count > 0 => {
                ECHO_COUNT.fetch_add(1, Ordering::Relaxed);
                let data = &rx_buf[..count];

                if phase == 0 {
                    // Phase 0: Interactive test commands
                    if data == b"PING\r" || data == b"PING\n" || data == b"PING" {
                        let c = ECHO_COUNT.load(Ordering::Relaxed);
                        write_str(&mut serial, b"PONG ");
                        // Write number as ASCII
                        let mut buf = [0u8; 20];
                        let mut n = c;
                        let mut pos = 20;
                        if n == 0 {
                            pos -= 1;
                            buf[pos] = b'0';
                        } else {
                            while n > 0 {
                                pos -= 1;
                                buf[pos] = b'0' + (n % 10) as u8;
                                n /= 10;
                            }
                        }
                        write_str(&mut serial, &buf[pos..]);
                        write_str(&mut serial, b"\r\n");
                    } else if data == b"STATS\r" || data == b"STATS\n" || data == b"STATS" {
                        write_str(&mut serial, b"USB Standalone Test Active\r\n");
                        let passed = PASS_COUNT.load(Ordering::Relaxed);
                        let failed = FAIL_COUNT.load(Ordering::Relaxed);
                        let echoes = ECHO_COUNT.load(Ordering::Relaxed);
                        write_str(&mut serial, b"Pass: ");
                        write_dec(&mut serial, passed);
                        write_str(&mut serial, b" Fail: ");
                        write_dec(&mut serial, failed);
                        write_str(&mut serial, b" Echoes: ");
                        write_dec(&mut serial, echoes);
                        write_str(&mut serial, b" Polls: ");
                        write_dec(&mut serial, poll_count);
                        write_str(&mut serial, b"\r\n");
                    } else if data == b"BYE\r" || data == b"BYE\n" || data == b"BYE" {
                        write_str(&mut serial, b"=== USB Test Summary ===\r\n");
                        write_str(&mut serial, b"Echo exchanges: ");
                        write_dec(&mut serial, ECHO_COUNT.load(Ordering::Relaxed));
                        write_str(&mut serial, b"\r\n");
                        write_str(&mut serial, b"Starting sustained poll test...\r\n");
                        phase = 1;
                        stress_target = poll_count + 10000;
                    } else {
                        // Echo back
                        let _ = serial.write(data);
                    }
                } else if phase == 1 {
                    // Phase 1: Sustained poll test (ignore input, just count polls)
                    // Already counting above
                }
            }
            _ => {}
        }

        // LED blink
        if poll_count % 1000 == 0 {
            led_state = !led_state;
            if led_state {
                led.set_high();
            } else {
                led.set_low();
            }
        }

        // Phase 1 completion
        if phase == 1 && poll_count >= stress_target {
            phase = 2;
            write_str(&mut serial, b"usb_sustained_poll: ");
            write_ok(&mut serial);

            write_str(&mut serial, b"\r\n=== ALL TESTS SUMMARY ===\r\n");
            let passed = PASS_COUNT.load(Ordering::Relaxed);
            let failed = FAIL_COUNT.load(Ordering::Relaxed);
            let total = passed + failed;
            write_str(&mut serial, b"SUMMARY: ");
            write_dec(&mut serial, passed);
            write_str(&mut serial, b"/");
            write_dec(&mut serial, total);
            write_str(&mut serial, b" passed\r\n");

            if failed == 0 {
                write_str(&mut serial, b"ALL TESTS PASSED\r\n");
            } else {
                write_str(&mut serial, b"FAILED: ");
                write_dec(&mut serial, failed);
                write_str(&mut serial, b" tests failed\r\n");
            }
            write_str(&mut serial, b"BYE DONE\r\n");
            phase = 3; // Done, just keep polling for USB alive
        }
    }
}

fn write_dec(serial: &mut usbd_serial::SerialPort<UsbBus<USB>>, mut n: usize) {
    if n == 0 {
        let _ = serial.write(b"0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut pos = 20;
    while n > 0 {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    let _ = serial.write(&buf[pos..]);
}
