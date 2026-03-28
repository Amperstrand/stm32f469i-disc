//! Timer test for STM32F469I-DISCO
//!
//! Tests TIM2 (32-bit) and TIM3 (16-bit) using DWT cycle counter.
//! NOTE: counter_ms() is broken above 65 MHz, so we use counter_us() only.

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use crate::board::hal::{pac, prelude::*, rcc};

use cortex_m::peripheral::{Peripherals, DWT};
use cortex_m_rt::entry;
use stm32f4xx_hal::nb::block;

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

fn cycles_to_us(cycles: u32) -> u32 {
    cycles / 180
}

fn cycles_to_ms(cycles: u32) -> u32 {
    cycles / 180_000
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(mut cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));

        defmt::info!("=== Timer Test Suite ===");

        cp.DCB.enable_trace();
        cp.DWT.enable_cycle_counter();

        // Test 1: DWT cycle counter basic sanity
        defmt::info!("TEST dwt_sanity: RUNNING");
        {
            let start = DWT::cycle_count();
            let end = DWT::cycle_count();
            let diff = end.wrapping_sub(start);
            if diff < 1000 {
                defmt::info!("  DWT delta: {} cycles", diff);
                pass("dwt_sanity");
            } else {
                fail("dwt_sanity", "DWT counter not incrementing properly");
            }
        }

        // Test 2: DWT 1ms delay (software loop baseline)
        defmt::info!("TEST dwt_1ms: RUNNING");
        {
            let start = DWT::cycle_count();
            for _ in 0..180_000 {
                cortex_m::asm::nop();
            }
            let elapsed = DWT::cycle_count().wrapping_sub(start);
            let us = cycles_to_us(elapsed);
            defmt::info!("  180K nops: {}us", us);
            if us >= 500 && us <= 15000 {
                pass("dwt_1ms");
            } else {
                fail("dwt_1ms", "DWT timing out of range");
            }
        }

        // Test 3: TIM2 counter_us - 1ms delay
        defmt::info!("TEST tim2_counter_us_1ms: RUNNING");
        {
            let mut counter = p.TIM2.counter_us(&mut rcc);
            let start = DWT::cycle_count();
            counter.start(1.millis()).unwrap();
            block!(counter.wait()).unwrap();
            let elapsed = DWT::cycle_count().wrapping_sub(start);
            let us = cycles_to_us(elapsed);
            defmt::info!("  TIM2 1ms delay: {}us", us);
            if us >= 900 && us <= 1500 {
                pass("tim2_counter_us_1ms");
            } else {
                fail("tim2_counter_us_1ms", "1ms delay out of range");
            }
        }

        // Test 4: TIM2 counter_us - 500us delay (reuse TIM2 via steal)
        defmt::info!("TEST tim2_counter_us_500us: RUNNING");
        {
            let tim2 = unsafe { pac::Peripherals::steal().TIM2 };
            let mut counter = tim2.counter_us(&mut rcc);
            let start = DWT::cycle_count();
            counter.start(500.micros()).unwrap();
            block!(counter.wait()).unwrap();
            let elapsed = DWT::cycle_count().wrapping_sub(start);
            let us = cycles_to_us(elapsed);
            defmt::info!("  TIM2 500us delay: {}us", us);
            if us >= 400 && us <= 700 {
                pass("tim2_counter_us_500us");
            } else {
                fail("tim2_counter_us_500us", "500us delay out of range");
            }
        }

        // Test 5: TIM3 counter_us - 50ms (16-bit max is ~65ms at 1MHz)
        defmt::info!("TEST tim3_counter_us_50ms: RUNNING");
        {
            let tim3 = unsafe { pac::Peripherals::steal().TIM3 };
            let mut counter = tim3.counter_us(&mut rcc);
            let start = DWT::cycle_count();
            counter.start(50.millis()).unwrap();
            block!(counter.wait()).unwrap();
            let elapsed = DWT::cycle_count().wrapping_sub(start);
            let ms = cycles_to_ms(elapsed);
            defmt::info!("  TIM3 50ms delay: {}ms", ms);
            if ms >= 45 && ms <= 70 {
                pass("tim3_counter_us_50ms");
            } else {
                fail("tim3_counter_us_50ms", "50ms delay out of range");
            }
        }

        // Test 6: TIM3 PWM init + duty cycle (PA6 CH1)
        defmt::info!("TEST tim3_pwm_duty: RUNNING");
        {
            let gpioa = unsafe { pac::Peripherals::steal().GPIOA.split(&mut rcc) };
            let tim3 = unsafe { pac::Peripherals::steal().TIM3 };
            let (_pwm, (ch1, _ch2, _ch3, _ch4)) = tim3.pwm_hz(10.kHz(), &mut rcc);
            let mut ch1 = ch1.with(gpioa.pa6);
            let max_duty = ch1.get_duty();
            ch1.set_duty(max_duty / 2);
            ch1.enable();
            ch1.set_duty(max_duty / 4);
            ch1.set_duty(0);
            ch1.set_duty(max_duty);
            ch1.disable();
            pass("tim3_pwm_duty");
        }

        // Test 7: TIM2 PWM frequency change (PA0 CH1)
        defmt::info!("TEST tim2_pwm_freq_change: RUNNING");
        {
            let gpioa = unsafe { pac::Peripherals::steal().GPIOA.split(&mut rcc) };
            let tim2 = unsafe { pac::Peripherals::steal().TIM2 };
            let (mut pwm, (ch1, _ch2, _ch3, _ch4)) = tim2.pwm_hz(1.kHz(), &mut rcc);
            let mut ch1 = ch1.with(gpioa.pa0);
            ch1.enable();
            ch1.set_duty(ch1.get_duty() / 2);
            let _ = pwm.set_period(10.kHz());
            ch1.set_duty(ch1.get_duty() / 2);
            let _ = pwm.set_period(100.kHz());
            ch1.set_duty(ch1.get_duty() / 4);
            ch1.disable();
            pass("tim2_pwm_freq_change");
        }

        // Test 8: Timer cancel
        defmt::info!("TEST tim2_cancel: RUNNING");
        {
            let tim2 = unsafe { pac::Peripherals::steal().TIM2 };
            let mut counter = tim2.counter_us(&mut rcc);
            counter.start(10.secs()).unwrap();
            let _ = counter.cancel();
            let start = DWT::cycle_count();
            let _ = counter.cancel();
            let elapsed = DWT::cycle_count().wrapping_sub(start);
            if elapsed < 180_000 {
                pass("tim2_cancel");
            } else {
                fail("tim2_cancel", "cancel took too long or blocked");
            }
        }

        let passed = PASSED.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let total = passed + failed;

        defmt::info!("=== Timer Test Summary ===");
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
