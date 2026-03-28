//! UART/USART test for STM32F469I-DISCO
//!
//! Tests USART1 on PA9 (TX) via ST-Link VCP.

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use crate::board::hal::{pac, prelude::*, rcc};

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

fn fail(name: &str, reason: &str) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!("TEST {}: FAIL {}", name, reason);
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let mut delay = cp.SYST.delay(&clocks);

        let gpioa = p.GPIOA.split(&mut rcc);

        defmt::info!("=== UART Test Suite ===");

        // Test 1: USART1 init at 115200 + TX write single byte
        defmt::info!("TEST usart1_init_115200: RUNNING");
        let mut tx: board::hal::serial::Tx<pac::USART1> =
            match p.USART1.tx(gpioa.pa9, 115200.bps(), &mut rcc) {
                Ok(tx) => {
                    pass("usart1_init_115200");
                    tx
                }
                Err(_) => {
                    fail("usart1_init_115200", "failed to init USART1 at 115200");
                    loop {
                        continue;
                    }
                }
            };

        // Test 2: TX write single byte
        defmt::info!("TEST usart1_tx_byte: RUNNING");
        match tx.write(b'U') {
            Ok(()) => pass("usart1_tx_byte"),
            Err(_) => fail("usart1_tx_byte", "write returned error"),
        }

        // Test 3: Formatted output
        defmt::info!("TEST usart1_fmt_write: RUNNING");
        match write!(tx, "FMT: pass={} fail={}\r\n", 1, 0) {
            Ok(()) => pass("usart1_fmt_write"),
            Err(_) => fail("usart1_fmt_write", "fmt write error"),
        }

        // Test 4: Multiple sequential bytes
        defmt::info!("TEST usart1_multi_write: RUNNING");
        {
            let mut ok = true;
            for i in 0..26u8 {
                if tx.write(b'A' + i).is_err() {
                    ok = false;
                    break;
                }
            }
            if tx.write(b'\r').is_err() {
                ok = false;
            }
            if tx.write(b'\n').is_err() {
                ok = false;
            }
            if ok {
                pass("usart1_multi_write");
            } else {
                fail("usart1_multi_write", "write failed on multi-write");
            }
        }

        delay.delay_ms(100u32);

        let passed = PASSED.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let total = passed + failed;

        defmt::info!("=== UART Test Summary ===");
        defmt::info!("SUMMARY: {}/{} passed", passed, total);

        if failed == 0 {
            defmt::info!("ALL TESTS PASSED");
        } else {
            defmt::error!("FAILED: {} tests failed", failed);
        }
    }

    loop {
        continue;
    }
}
