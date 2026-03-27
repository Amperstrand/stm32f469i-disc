//! Full SDRAM tests for STM32F469I-DISCO (16MB IS42S32400F-6)
//!
//! Exhaustive tests over ALL 16MB. Takes ~3-5 minutes.
//! For fast spot-checks, use test_sdram instead.
//!
//! Tests:
//!   1-2. Walking 1s/0s (32-bit, full 16MB each)
//!   3-4. Checkerboard / inverse (full 16MB)
//!   5-6. Address pattern / inverse (full 16MB)
//!   7. Random XOR-shift (full 16MB)
//!   8-11. Solid fills: 0x00, 0xFF, 0xAA, 0x55 (full 16MB)
//!   12. March C- (full 16MB)
//!   13. Boundary burst at 4K intervals
//!   14. Multi-pass random (3 passes, full 16MB)
//!   15-16. Byte/halfword access (first 64K/32K)

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use crate::board::hal::gpio::alt::fmc as alt;
use crate::board::hal::{pac, prelude::*, rcc};
use crate::board::sdram::{sdram_pins, Sdram};

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

struct XorShift32 {
    seed: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        XorShift32 {
            seed: if seed == 0 { 1 } else { seed },
        }
    }

    fn next(&mut self) -> u32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed
    }
}

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

fn fail(name: &str, addr: usize, expected: u32, got: u32) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!(
        "TEST {}: FAIL addr={:#010X} expected={:#010X} got={:#010X}",
        name,
        addr,
        expected,
        got
    );
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(cp)) = (pac::Peripherals::take(), Peripherals::take()) {
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

        defmt::info!("=== SDRAM Full Test Suite (exhaustive) ===");
        defmt::info!("Initializing SDRAM...");

        let sdram = Sdram::new(
            p.FMC,
            sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
            &clocks,
            &mut delay,
        );

        let words = sdram.words;
        let base = sdram.mem as usize;
        defmt::info!("SDRAM: base={:#010X} words={}", base, words);

        let ram: &mut [u32] = unsafe { core::slice::from_raw_parts_mut(sdram.mem, words) };

        // Test 1: Walking 1s (full 16MB × 32 bits)
        defmt::info!("TEST walking_1s: RUNNING");
        {
            let mut ok = true;
            for bit in 0..32 {
                let pattern = 1u32 << bit;
                for word in ram.iter_mut() {
                    *word = pattern;
                }
                for (i, word) in ram.iter().enumerate() {
                    if *word != pattern {
                        fail("walking_1s", base + i * 4, pattern, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
                defmt::info!("  walking_1s: bit {} done", bit);
            }
            if ok {
                pass("walking_1s");
            }
        }

        // Test 2: Walking 0s (full 16MB × 32 bits)
        defmt::info!("TEST walking_0s: RUNNING");
        {
            let mut ok = true;
            for bit in 0..32 {
                let pattern = !(1u32 << bit);
                for word in ram.iter_mut() {
                    *word = pattern;
                }
                for (i, word) in ram.iter().enumerate() {
                    if *word != pattern {
                        fail("walking_0s", base + i * 4, pattern, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
                defmt::info!("  walking_0s: bit {} done", bit);
            }
            if ok {
                pass("walking_0s");
            }
        }

        // Test 3: Checkerboard
        defmt::info!("TEST checkerboard: RUNNING");
        {
            let mut ok = true;
            for word in ram.iter_mut() {
                *word = 0xAAAAAAAA;
            }
            for (i, word) in ram.iter().enumerate() {
                if *word != 0xAAAAAAAA {
                    fail("checkerboard", base + i * 4, 0xAAAAAAAA, *word);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("checkerboard");
            }
        }

        // Test 4: Inverse checkerboard
        defmt::info!("TEST inv_checkerboard: RUNNING");
        {
            let mut ok = true;
            for word in ram.iter_mut() {
                *word = 0x55555555;
            }
            for (i, word) in ram.iter().enumerate() {
                if *word != 0x55555555 {
                    fail("inv_checkerboard", base + i * 4, 0x55555555, *word);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("inv_checkerboard");
            }
        }

        // Test 5: Address pattern
        defmt::info!("TEST addr_pattern: RUNNING");
        {
            let mut ok = true;
            for (i, word) in ram.iter_mut().enumerate() {
                *word = (base + i * 4) as u32;
            }
            for (i, word) in ram.iter().enumerate() {
                let expected = (base + i * 4) as u32;
                if *word != expected {
                    fail("addr_pattern", base + i * 4, expected, *word);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("addr_pattern");
            }
        }

        // Test 6: Inverse address pattern
        defmt::info!("TEST inv_addr_pattern: RUNNING");
        {
            let mut ok = true;
            for (i, word) in ram.iter_mut().enumerate() {
                *word = !((base + i * 4) as u32);
            }
            for (i, word) in ram.iter().enumerate() {
                let expected = !((base + i * 4) as u32);
                if *word != expected {
                    fail("inv_addr_pattern", base + i * 4, expected, *word);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("inv_addr_pattern");
            }
        }

        // Test 7: Random XOR-shift (full 16MB)
        defmt::info!("TEST random_xorshift: RUNNING");
        {
            let mut rng = XorShift32::new(0xDEADBEEF);
            for word in ram.iter_mut() {
                *word = rng.next();
            }
            let mut ok = true;
            let mut rng = XorShift32::new(0xDEADBEEF);
            for (i, word) in ram.iter().enumerate() {
                let expected = rng.next();
                if *word != expected {
                    fail("random_xorshift", base + i * 4, expected, *word);
                    ok = false;
                    break;
                }
                if (i & 0x3FFFF) == 0 {
                    defmt::info!("  random_xorshift: verified {:#010X}", base + i * 4);
                }
            }
            if ok {
                pass("random_xorshift");
            }
        }

        // Test 8-11: Solid fills
        defmt::info!("TEST solid_fills: RUNNING");
        {
            let mut ok = true;
            let fills: [(u32, &str); 4] = [
                (0x00000000, "zero"),
                (0xFFFFFFFF, "ones"),
                (0xAAAAAAAA, "aa"),
                (0x55555555, "55"),
            ];
            for &(fill, _label) in &fills {
                for word in ram.iter_mut() {
                    *word = fill;
                }
                for (i, word) in ram.iter().enumerate() {
                    if *word != fill {
                        fail("solid_fills", base + i * 4, fill, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
                defmt::info!("  solid_fills: {:#010X} done", fill);
            }
            if ok {
                pass("solid_fills");
            }
        }

        // Test 12: March C- (full 16MB)
        defmt::info!("TEST march_c: RUNNING");
        {
            let mut ok = true;
            for word in ram.iter_mut() {
                *word = 0;
            }
            defmt::info!("  march_c: w0 done");
            // Up: r0 w1
            for (i, word) in ram.iter_mut().enumerate() {
                if *word != 0 {
                    fail("march_c", base + i * 4, 0, *word);
                    ok = false;
                    break;
                }
                *word = 0xFFFFFFFF;
            }
            defmt::info!("  march_c: up r0/w1 done");
            if ok {
                // Down: r1 w0
                for word in ram.iter_mut().rev() {
                    if *word != 0xFFFFFFFF {
                        let i = word as *const u32 as usize;
                        fail("march_c", i, 0xFFFFFFFF, *word);
                        ok = false;
                        break;
                    }
                    *word = 0;
                }
                defmt::info!("  march_c: down r1/w0 done");
            }
            if ok {
                // Up: r0 w1
                for (i, word) in ram.iter_mut().enumerate() {
                    if *word != 0 {
                        fail("march_c", base + i * 4, 0, *word);
                        ok = false;
                        break;
                    }
                    *word = 0xFFFFFFFF;
                }
                defmt::info!("  march_c: up2 r0/w1 done");
            }
            if ok {
                // Up: r1 w0
                for (i, word) in ram.iter_mut().enumerate() {
                    if *word != 0xFFFFFFFF {
                        fail("march_c", base + i * 4, 0xFFFFFFFF, *word);
                        ok = false;
                        break;
                    }
                    *word = 0;
                }
                defmt::info!("  march_c: up3 r1/w0 done");
            }
            if ok {
                for (i, word) in ram.iter().enumerate() {
                    if *word != 0 {
                        fail("march_c", base + i * 4, 0, *word);
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                pass("march_c");
            }
        }

        // Test 13: Boundary burst at 4K intervals (64 boundaries, 16 words each)
        defmt::info!("TEST boundary_burst: RUNNING");
        {
            let mut ok = true;
            let boundary_size = 4096;
            let num_boundaries = core::cmp::min(words / boundary_size, 4096);
            for b in 0..num_boundaries {
                let offset = b * boundary_size;
                let pattern = 0xDEAD0000 | (b as u32);
                for j in 0..16 {
                    ram[offset + j] = pattern;
                }
            }
            for b in 0..num_boundaries {
                let offset = b * boundary_size;
                let pattern = 0xDEAD0000 | (b as u32);
                for j in 0..16 {
                    if ram[offset + j] != pattern {
                        fail(
                            "boundary_burst",
                            base + (offset + j) * 4,
                            pattern,
                            ram[offset + j],
                        );
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("boundary_burst");
            }
        }

        // Test 14: Multi-pass random (3 passes, full 16MB)
        defmt::info!("TEST multi_pass_random: RUNNING");
        {
            let mut ok = true;
            for pass_num in 0..3 {
                let seed = 0xCAFEBABE + pass_num * 0x11111111;
                let mut rng = XorShift32::new(seed);
                for word in ram.iter_mut() {
                    *word = rng.next();
                }
                let mut rng = XorShift32::new(seed);
                for (i, word) in ram.iter().enumerate() {
                    let expected = rng.next();
                    if *word != expected {
                        fail("multi_pass_random", base + i * 4, expected, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
                defmt::info!("  multi_pass_random: pass {} OK", pass_num + 1);
            }
            if ok {
                pass("multi_pass_random");
            }
        }

        // Test 15: Byte-level (first 64K)
        defmt::info!("TEST byte_level: RUNNING");
        {
            let mut ok = true;
            let ram_bytes: &mut [u8] =
                unsafe { core::slice::from_raw_parts_mut(sdram.mem as *mut u8, words * 4) };
            let byte_count = 65536;
            for (i, byte) in ram_bytes[..byte_count].iter_mut().enumerate() {
                *byte = (i & 0xFF) as u8;
            }
            for (i, byte) in ram_bytes[..byte_count].iter().enumerate() {
                let expected = (i & 0xFF) as u8;
                if *byte != expected {
                    fail("byte_level", base + i, expected as u32, *byte as u32);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("byte_level");
            }
        }

        // Test 16: Halfword-level (first 32K)
        defmt::info!("TEST halfword_level: RUNNING");
        {
            let mut ok = true;
            let hw_count = 16384;
            let ram_hw: &mut [u16] =
                unsafe { core::slice::from_raw_parts_mut(sdram.mem as *mut u16, hw_count) };
            for (i, hw) in ram_hw.iter_mut().enumerate() {
                *hw = ((i & 0xFFFF) as u16).wrapping_add(1);
            }
            for (i, hw) in ram_hw.iter().enumerate() {
                let expected = ((i & 0xFFFF) as u16).wrapping_add(1);
                if *hw != expected {
                    fail("halfword_level", base + i * 2, expected as u32, *hw as u32);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("halfword_level");
            }
        }

        // Summary
        let passed = PASSED.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let total = passed + failed;

        defmt::info!("=== SDRAM Full Test Summary ===");
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
