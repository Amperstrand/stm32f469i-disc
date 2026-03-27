//! Fast SDRAM spot-check tests for STM32F469I-DISCO (16MB IS42S32400F-6)
//!
//! Strategy: thoroughly test first 256KB (catches init/timing bugs),
//! then spot-check at boundaries throughout all 16MB.
//! Completes in ~5-10 seconds.
//!
//! For full 16MB exhaustive testing, use test_sdram_full.
//!
//! Tests:
//!   1. First 256K: checkerboard / inverse checkerboard
//!   2. First 256K: address pattern / inverse
//!   3. First 256K: random XOR-shift
//!   4. First 256K: solid fills (0x00, 0xFF, 0xAA, 0x55)
//!   5. First 256K: walking 1s/0s (32 bits, on small window only)
//!   6. First 256K: March C- (small window)
//!   7. Boundary spots: write/read at 16 evenly spaced 1MB regions
//!   8. Scattered spots: random probes across all 16MB
//!   9. End-of-ram: last 64K random test
//!   10. Byte/halfword access on first 4K

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

        defmt::info!("=== SDRAM Fast Test Suite ===");
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

        // Thorough window: first 256KB = 65536 words
        const WIN: usize = 65536;

        // === SECTION 1: Thorough tests on first 256KB ===

        // Test 1: Checkerboard on window
        defmt::info!("TEST checkerboard: RUNNING");
        {
            let mut ok = true;
            for word in ram[..WIN].iter_mut() {
                *word = 0xAAAAAAAA;
            }
            for (i, word) in ram[..WIN].iter().enumerate() {
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

        // Test 2: Inverse checkerboard
        defmt::info!("TEST inv_checkerboard: RUNNING");
        {
            let mut ok = true;
            for word in ram[..WIN].iter_mut() {
                *word = 0x55555555;
            }
            for (i, word) in ram[..WIN].iter().enumerate() {
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

        // Test 3: Address pattern
        defmt::info!("TEST addr_pattern: RUNNING");
        {
            let mut ok = true;
            for (i, word) in ram[..WIN].iter_mut().enumerate() {
                *word = (base + i * 4) as u32;
            }
            for (i, word) in ram[..WIN].iter().enumerate() {
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

        // Test 4: Inverse address pattern
        defmt::info!("TEST inv_addr_pattern: RUNNING");
        {
            let mut ok = true;
            for (i, word) in ram[..WIN].iter_mut().enumerate() {
                *word = !((base + i * 4) as u32);
            }
            for (i, word) in ram[..WIN].iter().enumerate() {
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

        // Test 5: Random XOR-shift on window
        defmt::info!("TEST random_xorshift: RUNNING");
        {
            let mut rng = XorShift32::new(0xDEADBEEF);
            for word in ram[..WIN].iter_mut() {
                *word = rng.next();
            }
            let mut ok = true;
            let mut rng = XorShift32::new(0xDEADBEEF);
            for (i, word) in ram[..WIN].iter().enumerate() {
                let expected = rng.next();
                if *word != expected {
                    fail("random_xorshift", base + i * 4, expected, *word);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("random_xorshift");
            }
        }

        // Test 6: Walking 1s (small window only - 32 bits × 64K words is fast)
        defmt::info!("TEST walking_1s: RUNNING");
        {
            let mut ok = true;
            for bit in 0..32 {
                let pattern = 1u32 << bit;
                for word in ram[..WIN].iter_mut() {
                    *word = pattern;
                }
                for (i, word) in ram[..WIN].iter().enumerate() {
                    if *word != pattern {
                        fail("walking_1s", base + i * 4, pattern, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("walking_1s");
            }
        }

        // Test 7: Walking 0s (small window)
        defmt::info!("TEST walking_0s: RUNNING");
        {
            let mut ok = true;
            for bit in 0..32 {
                let pattern = !(1u32 << bit);
                for word in ram[..WIN].iter_mut() {
                    *word = pattern;
                }
                for (i, word) in ram[..WIN].iter().enumerate() {
                    if *word != pattern {
                        fail("walking_0s", base + i * 4, pattern, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("walking_0s");
            }
        }

        // Test 8: Solid fills
        defmt::info!("TEST solid_fills: RUNNING");
        {
            let mut ok = true;
            let fills: [u32; 4] = [0x00000000, 0xFFFFFFFF, 0xAAAAAAAA, 0x55555555];
            for &fill in &fills {
                for word in ram[..WIN].iter_mut() {
                    *word = fill;
                }
                for (i, word) in ram[..WIN].iter().enumerate() {
                    if *word != fill {
                        fail("solid_fills", base + i * 4, fill, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("solid_fills");
            }
        }

        // Test 9: March C- (small window)
        defmt::info!("TEST march_c: RUNNING");
        {
            let mut ok = true;
            for word in ram[..WIN].iter_mut() {
                *word = 0;
            }
            // Up: r0 w1
            for (i, word) in ram[..WIN].iter_mut().enumerate() {
                if *word != 0 {
                    fail("march_c", base + i * 4, 0, *word);
                    ok = false;
                    break;
                }
                *word = 0xFFFFFFFF;
            }
            if ok {
                // Down: r1 w0
                for word in ram[..WIN].iter_mut().rev() {
                    if *word != 0xFFFFFFFF {
                        let i = word as *const u32 as usize;
                        fail("march_c", i, 0xFFFFFFFF, *word);
                        ok = false;
                        break;
                    }
                    *word = 0;
                }
            }
            if ok {
                // Up: r0 w1
                for (i, word) in ram[..WIN].iter_mut().enumerate() {
                    if *word != 0 {
                        fail("march_c", base + i * 4, 0, *word);
                        ok = false;
                        break;
                    }
                    *word = 0xFFFFFFFF;
                }
            }
            if ok {
                // Up: r1 w0
                for (i, word) in ram[..WIN].iter_mut().enumerate() {
                    if *word != 0xFFFFFFFF {
                        fail("march_c", base + i * 4, 0xFFFFFFFF, *word);
                        ok = false;
                        break;
                    }
                    *word = 0;
                }
            }
            if ok {
                for (i, word) in ram[..WIN].iter().enumerate() {
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

        // === SECTION 2: Spot-checks across all 16MB ===

        // Test 10: Boundary spots - 16 evenly spaced 1MB regions, 1024 words each
        defmt::info!("TEST boundary_spots: RUNNING");
        {
            let mut ok = true;
            let region_size = 1024; // words per region
            let num_regions = 16;
            let region_stride = words / num_regions;

            for r in 0..num_regions {
                let offset = r * region_stride;
                let pattern = 0xFEED0000 | (r as u32);
                let end = core::cmp::min(offset + region_size, words);
                for word in ram[offset..end].iter_mut() {
                    *word = pattern;
                }
            }
            for r in 0..num_regions {
                let offset = r * region_stride;
                let pattern = 0xFEED0000 | (r as u32);
                let end = core::cmp::min(offset + region_size, words);
                for (i, word) in ram[offset..end].iter().enumerate() {
                    if *word != pattern {
                        fail("boundary_spots", base + (offset + i) * 4, pattern, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("boundary_spots");
            }
        }

        // Test 11: Scattered random probes - 32 random 4K blocks across 16MB
        defmt::info!("TEST scattered_random: RUNNING");
        {
            let mut ok = true;
            let block_words = 1024; // 4K per block
            let mut rng = XorShift32::new(0xBEEFCAFE);

            for _probe in 0..32 {
                let offset = (rng.next() as usize) % (words - block_words);
                let seed_val = rng.next();

                let mut block_rng = XorShift32::new(seed_val);
                for word in ram[offset..offset + block_words].iter_mut() {
                    *word = block_rng.next();
                }

                let mut block_rng = XorShift32::new(seed_val);
                for (i, word) in ram[offset..offset + block_words].iter().enumerate() {
                    let expected = block_rng.next();
                    if *word != expected {
                        fail("scattered_random", base + (offset + i) * 4, expected, *word);
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("scattered_random");
            }
        }

        // Test 12: Last 64K
        defmt::info!("TEST end_of_ram: RUNNING");
        {
            let mut ok = true;
            let last = 16384; // 64K in words
            let start = words - last;
            let mut rng = XorShift32::new(0x12345678);
            for word in ram[start..].iter_mut() {
                *word = rng.next();
            }
            let mut rng = XorShift32::new(0x12345678);
            for (i, word) in ram[start..].iter().enumerate() {
                let expected = rng.next();
                if *word != expected {
                    fail("end_of_ram", base + (start + i) * 4, expected, *word);
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("end_of_ram");
            }
        }

        // Test 13: Byte-level on first 4K
        defmt::info!("TEST byte_level: RUNNING");
        {
            let mut ok = true;
            let ram_bytes: &mut [u8] =
                unsafe { core::slice::from_raw_parts_mut(sdram.mem as *mut u8, 4096) };
            for (i, byte) in ram_bytes.iter_mut().enumerate() {
                *byte = (i & 0xFF) as u8;
            }
            for (i, byte) in ram_bytes.iter().enumerate() {
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

        // Test 14: Halfword-level on first 4K
        defmt::info!("TEST halfword_level: RUNNING");
        {
            let mut ok = true;
            let ram_hw: &mut [u16] =
                unsafe { core::slice::from_raw_parts_mut(sdram.mem as *mut u16, 2048) };
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

        defmt::info!("=== SDRAM Fast Test Summary ===");
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
