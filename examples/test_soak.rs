//! Continuous soak test for STM32F469I-DISCO
//!
//! Runs indefinitely: LED heartbeat + SDRAM pattern write/verify cycles.
//! No SUMMARY line — killed by timeout in run_tests.sh.
//! Monitor via defmt RTT output.

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::gpio::alt::fmc as alt;
use board::hal::{pac, prelude::*, rcc};
use board::sdram::{sdram_pins, Sdram};

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use core::slice;

const SOAK_SIZE: usize = 262144; // 1MB window (256K u32 words)

#[entry]
fn main() -> ! {
    if let (Some(p), Some(_cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let mut delay = cortex_m::Peripherals::take().unwrap().SYST.delay(&clocks);

        defmt::info!("=== Soak Test ===");
        defmt::info!("Continuous SDRAM stress + LED heartbeat");
        defmt::info!("Press Ctrl+C to stop");

        let gpioc = p.GPIOC.split(&mut rcc);
        let gpiod = p.GPIOD.split(&mut rcc);
        let gpioe = p.GPIOE.split(&mut rcc);
        let gpiof = p.GPIOF.split(&mut rcc);
        let gpiog = p.GPIOG.split(&mut rcc);
        let gpioh = p.GPIOH.split(&mut rcc);
        let gpioi = p.GPIOI.split(&mut rcc);

        let sdram = Sdram::new(
            p.FMC,
            sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
            &clocks,
            &mut delay,
        );

        let words = sdram.words;
        let win = core::cmp::min(SOAK_SIZE, words);
        let ram: &mut [u32] = unsafe { slice::from_raw_parts_mut(sdram.mem, words) };

        let mut led = gpiog.pg6.into_push_pull_output();
        let mut count: u32 = 0;
        let mut errors: u32 = 0;

        defmt::info!("Soak: {} words, window={}", words, win);

        loop {
            // LED heartbeat
            led.toggle();
            delay.delay_ms(500u32);
            count += 1;

            // SDRAM pattern: alternate 0xAAAAAAAA / 0x55555555
            let pattern: u32 = if count.is_multiple_of(2) {
                0xAAAAAAAA
            } else {
                0x55555555
            };
            for w in ram[..win].iter_mut() {
                *w = pattern;
            }
            let mut ok = true;
            for w in ram[..win].iter() {
                if *w != pattern {
                    ok = false;
                    break;
                }
            }
            if !ok {
                errors += 1;
                defmt::error!("SOAK ERROR at cycle {}", count);
            }

            // Also do address pattern every 10 cycles
            if count.is_multiple_of(10) {
                for (i, w) in ram[..win].iter_mut().enumerate() {
                    *w = (i as u32) ^ 0xDEADBEEF;
                }
                let mut addr_ok = true;
                for (i, w) in ram[..win].iter().enumerate() {
                    if *w != ((i as u32) ^ 0xDEADBEEF) {
                        addr_ok = false;
                        break;
                    }
                }
                if !addr_ok {
                    errors += 1;
                    defmt::error!("SOAK ADDR ERROR at cycle {}", count);
                }
            }

            if count.is_multiple_of(120) {
                defmt::info!("soak: {} heartbeats, {} errors", count, errors);
            }
        }
    }

    loop {
        continue;
    }
}
