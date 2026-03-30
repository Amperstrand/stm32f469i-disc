//! Continuous soak test for STM32F469I-DISCO SDRAM.
//!
//! Runs indefinitely: LED heartbeat + rotating test patterns on full 16MB
//! with periodic bit-fade (DRAM retention) checks. No SUMMARY line — killed
//! by timeout. Monitor via defmt RTT output.
//!
//! Based on memtest86+ best practices:
//! - Rotating patterns (checkerboard, address, random, March C-) catch
//!   different fault classes across thermal cycling
//! - Bit fade (fill, delay, verify) catches DRAM capacitor retention faults
//!   that immediate readback cannot detect
//!
//! Run explicitly: `./run_tests.sh test_soak` or flash manually.
//! NOT included in `run_tests.sh all` due to long runtime.

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

const BIT_FADE_DELAY_MS: u32 = 5000;

struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        Self {
            state: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let mut delay = cp.SYST.delay(&clocks);

        defmt::info!("=== SDRAM Soak Test ===");
        defmt::info!("Full 16MB, rotating patterns + bit fade ({}ms delay)", BIT_FADE_DELAY_MS);
        defmt::info!("Run indefinitely \u{2014} kill with Ctrl+C or timeout");

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
        let ram: &mut [u32] = unsafe { slice::from_raw_parts_mut(sdram.mem, words) };
        let base = sdram.mem as usize;

        let mut led = gpiog.pg6.into_push_pull_output();
        let mut cycle: u32 = 0;
        let mut errors: u32 = 0;
        let mut pattern_passes: [u32; 6] = [0; 6];
        let mut rng = XorShift32::new(0xDEADBEEF);

        defmt::info!("Soak: {} words ({} MB)", words, words * 4 / 1_000_000);

        loop {
            led.toggle();
            cycle += 1;

            match cycle % 7 {
                0 => {
                    let pattern = 0xAAAAAAAA;
                    for word in ram.iter_mut() {
                        *word = pattern;
                    }
                    for (i, word) in ram.iter().enumerate() {
                        if *word != pattern {
                            errors += 1;
                            defmt::error!("checkerboard_0xAA error at cycle {} addr={:#010X}", cycle, base + i * 4);
                            break;
                        }
                    }
                    pattern_passes[0] += 1;
                }
                1 => {
                    let pattern = 0x55555555;
                    for word in ram.iter_mut() {
                        *word = pattern;
                    }
                    for (i, word) in ram.iter().enumerate() {
                        if *word != pattern {
                            errors += 1;
                            defmt::error!("checkerboard_0x55 error at cycle {} addr={:#010X}", cycle, base + i * 4);
                            break;
                        }
                    }
                    pattern_passes[1] += 1;
                }
                2 => {
                    for (i, word) in ram.iter_mut().enumerate() {
                        *word = (base + i * 4) as u32;
                    }
                    for (i, word) in ram.iter().enumerate() {
                        let expected = (base + i * 4) as u32;
                        if *word != expected {
                            errors += 1;
                            defmt::error!("addr_pattern error at cycle {} addr={:#010X}", cycle, base + i * 4);
                            break;
                        }
                    }
                    pattern_passes[2] += 1;
                }
                3 => {
                    let seed = rng.next();
                    let mut write_rng = XorShift32::new(seed);
                    for word in ram.iter_mut() {
                        *word = write_rng.next();
                    }
                    let mut read_rng = XorShift32::new(seed);
                    for (i, word) in ram.iter().enumerate() {
                        let expected = read_rng.next();
                        if *word != expected {
                            errors += 1;
                            defmt::error!("random error at cycle {} addr={:#010X} expected={:#010X} got={:#010X}", cycle, base + i * 4, expected, *word);
                            break;
                        }
                    }
                    pattern_passes[3] += 1;
                }
                4 => {
                    for word in ram.iter_mut() {
                        *word = 0;
                    }
                    let mut ok = true;
                    for (i, word) in ram.iter_mut().enumerate() {
                        if *word != 0 {
                            defmt::error!("march_c up error at cycle {} addr={:#010X}", cycle, base + i * 4);
                            ok = false;
                            break;
                        }
                        *word = 0xFFFFFFFF;
                    }
                    if ok {
                        for word in ram.iter_mut().rev() {
                            if *word != 0xFFFFFFFF {
                                defmt::error!("march_c down error at cycle {}", cycle);
                                ok = false;
                                break;
                            }
                            *word = 0;
                        }
                    }
                    if !ok {
                        errors += 1;
                    }
                    pattern_passes[4] += 1;
                }
                5 => {
                    let seed = rng.next();
                    let mut write_rng = XorShift32::new(seed);
                    for word in ram.iter_mut() {
                        *word = write_rng.next();
                    }
                    defmt::info!("bit fade: filled, sleeping {}ms...", BIT_FADE_DELAY_MS);
                    delay.delay_ms(BIT_FADE_DELAY_MS);
                    let mut read_rng = XorShift32::new(seed);
                    for (i, word) in ram.iter().enumerate() {
                        let expected = read_rng.next();
                        if *word != expected {
                            errors += 1;
                            defmt::error!("BIT FADE error at cycle {} addr={:#010X} expected={:#010X} got={:#010X}", cycle, base + i * 4, expected, *word);
                            break;
                        }
                    }
                    pattern_passes[5] += 1;
                }
                _ => {
                    defmt::info!("soak: cycle {} heartbeat, {} errors", cycle, errors);
                }
            }

            if cycle.is_multiple_of(840) {
                defmt::info!(
                    "soak stats: cycles={} errors={} chk_aa={} chk_55={} addr={} rand={} march={} fade={}",
                    cycle,
                    errors,
                    pattern_passes[0],
                    pattern_passes[1],
                    pattern_passes[2],
                    pattern_passes[3],
                    pattern_passes[4],
                    pattern_passes[5],
                );
            }

            delay.delay_ms(10u32);
        }
    }

    loop {
        continue;
    }
}
