//! HIL test: SDRAM read/write verification on the STM32F469I-DISCO board.
//!
//! Initializes the on-board SDRAM, writes a pseudo-random pattern (XorShift32)
//! across all 16 MB, reads back and verifies. Prints HIL_RESULT and sleeps.
//!
//! Run: `timeout 120 probe-rs run --chip STM32F469NIHx target/.../fmc_sdram_test`

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use core::slice;

use stm32f469i_disc as board;

use crate::board::hal::gpio::alt::fmc as alt;
use crate::board::hal::{pac, prelude::*, rcc};
use crate::board::sdram::{sdram_pins, Sdram};

use cortex_m::peripheral::Peripherals;

use cortex_m_rt::entry;

struct XorShift32 {
    seed: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        XorShift32 { seed }
    }

    fn next(&mut self) -> u32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed
    }
}

#[entry]
fn main() -> ! {
    let result = run_test();
    defmt::info!("HIL_RESULT:fmc_sdram_test:{}", result);
    loop {
        cortex_m::asm::wfi();
    }
}

fn run_test() -> &'static str {
    let (p, cp) = match (pac::Peripherals::take(), Peripherals::take()) {
        (Some(p), Some(cp)) => (p, cp),
        _ => return "FAIL:peripherals_take_failed",
    };

    let rcc = p.RCC.constrain();
    let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
    let clocks = rcc.clocks;
    let mut delay = cp.SYST.delay(&clocks);

    let gpioc = p.GPIOC.split(&mut rcc);
    let gpiod = p.GPIOD.split(&mut rcc);
    let gpioe = p.GPIOE.split(&mut rcc);
    let gpiof = p.GPIOF.split(&mut rcc);
    let gpiog = p.GPIOG.split(&mut rcc);
    let gpioh = p.GPIOH.split(&mut rcc);
    let gpioi = p.GPIOI.split(&mut rcc);

    defmt::info!("Initializing SDRAM...\r");
    let sdram = Sdram::new(
        p.FMC,
        sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
        &clocks,
        &mut delay,
    );
    let ram = unsafe { slice::from_raw_parts_mut(sdram.mem, sdram.words) };
    let total_words = sdram.words;

    defmt::info!("Testing SDRAM ({} words)...\r", total_words);

    let seed: u32 = 0x8675309D;
    let mut pattern = XorShift32::new(seed);
    let mut errors: u32 = 0;

    for res in ram.iter_mut().take(total_words) {
        *res = pattern.next();
    }

    pattern = XorShift32::new(seed);
    for (addr, res) in ram.iter_mut().enumerate().take(total_words) {
        let val = pattern.next();
        if *res != val {
            errors += 1;
            if errors <= 5 {
                defmt::info!(
                    "Error: {:X} -> {:X} != {:X}\r",
                    (sdram.mem as usize) + addr,
                    val,
                    *res
                );
            }
            if errors >= 10 {
                defmt::info!("Too many errors, stopping at word {}\r", addr);
                break;
            }
        }
    }

    if errors == 0 {
        defmt::info!("SDRAM OK: {} words verified\r", total_words);
        "PASS"
    } else {
        defmt::info!("SDRAM FAIL: {} errors in {} words\r", errors, total_words);
        "FAIL:data_mismatch"
    }
}
