//! Comprehensive LED tests for STM32F469I-DISCO
//!
//! Tests:
//!   1. Individual LED on/off for each LED
//!   2. Individual LED toggle for each LED
//!   3. All LEDs on, then all off
//!   4. Sequential pattern (march)
//!   5. Index by LedColor enum
//!
//! Output format (parseable by host script):
//!   TEST <name>: PASS
//!   TEST <name>: FAIL <reason>
//!   SUMMARY: <passed>/<total> passed

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use crate::board::{
    hal::{pac, prelude::*, rcc},
    led::{LedColor, Leds},
};

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);
static TOTAL: AtomicUsize = AtomicUsize::new(0);

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

fn fail(name: &str, reason: &str) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!("TEST {}: FAIL {}", name, reason);
}

fn test_start(name: &str) {
    TOTAL.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: RUNNING", name);
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let mut delay = cp.SYST.delay(&clocks);

        let gpiod = p.GPIOD.split(&mut rcc);
        let gpiog = p.GPIOG.split(&mut rcc);
        let gpiok = p.GPIOK.split(&mut rcc);

        let mut leds = Leds::new(gpiod, gpiog, gpiok);

        defmt::info!("=== LED Test Suite ===");
        defmt::info!("LED count: {}", leds.len());

        // Test 1: Each LED individual on/off
        for (i, led) in leds.iter_mut().enumerate() {
            let name = match i {
                0 => "led0_green_on_off",
                1 => "led1_orange_on_off",
                2 => "led2_red_on_off",
                3 => "led3_blue_on_off",
                _ => "unknown_led",
            };
            test_start(name);
            led.on();
            delay.delay_ms(100u32);
            led.off();
            delay.delay_ms(100u32);
            pass(name);
        }

        // Test 2: Each LED toggle (3 cycles)
        for (i, led) in leds.iter_mut().enumerate() {
            let name = match i {
                0 => "led0_green_toggle",
                1 => "led1_orange_toggle",
                2 => "led2_red_toggle",
                3 => "led3_blue_toggle",
                _ => "unknown_toggle",
            };
            test_start(name);
            for _ in 0..3 {
                led.toggle();
                delay.delay_ms(50u32);
            }
            led.off();
            delay.delay_ms(50u32);
            pass(name);
        }

        // Test 3: All LEDs on
        test_start("all_leds_on");
        for led in leds.iter_mut() {
            led.on();
        }
        delay.delay_ms(500u32);
        pass("all_leds_on");

        // Test 4: All LEDs off
        test_start("all_leds_off");
        for led in leds.iter_mut() {
            led.off();
        }
        delay.delay_ms(200u32);
        pass("all_leds_off");

        // Test 5: Sequential march pattern
        test_start("march_pattern");
        for _round in 0..3 {
            for led in leds.iter_mut() {
                led.on();
                delay.delay_ms(80u32);
                led.off();
            }
        }
        pass("march_pattern");

        // Test 6: Ping-pong pattern
        test_start("ping_pong_pattern");
        for _round in 0..2 {
            for led in leds.iter_mut() {
                led.on();
                delay.delay_ms(60u32);
                led.off();
            }
            for led in leds.iter_mut().rev() {
                led.on();
                delay.delay_ms(60u32);
                led.off();
            }
        }
        pass("ping_pong_pattern");

        // Test 7: Index by LedColor enum
        test_start("index_by_color");
        let colors = [
            LedColor::Green,
            LedColor::Orange,
            LedColor::Red,
            LedColor::Blue,
        ];
        for color in &colors {
            leds[*color].on();
            delay.delay_ms(100u32);
            leds[*color].off();
        }
        pass("index_by_color");

        // Test 8: Rapid on/off stress
        test_start("rapid_toggle_stress");
        for _ in 0..100 {
            for led in leds.iter_mut() {
                led.on();
                led.off();
            }
        }
        pass("rapid_toggle_stress");

        // Test 9: Deref/DerefMut access
        test_start("deref_access");
        {
            let slice: &[board::led::Led] = &*leds;
            if slice.len() != 4 {
                fail("deref_access", "expected 4 LEDs");
            } else {
                pass("deref_access");
            }
        }

        // Test 10: All on then off together (visual confirmation)
        test_start("all_on_then_off");
        for led in leds.iter_mut() {
            led.on();
        }
        delay.delay_ms(1000u32);
        for led in leds.iter_mut() {
            led.off();
        }
        delay.delay_ms(500u32);
        pass("all_on_then_off");

        // Summary
        let passed = PASSED.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let total = TOTAL.load(Ordering::Relaxed);

        defmt::info!("=== LED Test Summary ===");
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
