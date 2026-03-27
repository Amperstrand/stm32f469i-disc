//! HIL test: SDIO clock speed sweep.
//!
//! **Optional test** – run only when you need to characterize your SD card.
//! Repeated runs may stress the card; use sparingly. See `docs/SDIO-CLOCK-SPEEDS.md`.
//!
//! After init at 400 kHz and 500 ms stabilization, tries each of 1, 4, 8, 12, 24 MHz
//! with a short read test (256 blocks), then runs the full 10 MiB test at 1 MHz.
//! Prints HIL_RESULT and halts. Gracefully skips if no card is detected.
//!
//! Build/run (requires `sdio-speed-test` feature):
//! - Full sweep: `cargo run --example sdio_speed_sweep --features sdio-speed-test`
//! - One frequency: `cargo run --example sdio_speed_sweep --features sdio-speed-test,sweep-4mhz`

#![no_main]
#![no_std]

use cortex_m::peripheral::Peripherals;
use cortex_m::peripheral::DWT;
use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_probe as _;

use board::hal::{pac, prelude::*, rcc};
use board::sdio;
use board::sdram::{split_sdram_pins, Sdram};
use stm32f469i_disc as board;

const SWEEP_BLOCKS: u32 = 256;
#[cfg(not(any(
    feature = "sweep-1mhz",
    feature = "sweep-4mhz",
    feature = "sweep-8mhz",
    feature = "sweep-12mhz",
    feature = "sweep-24mhz"
)))]
const FULL_TEST_BLOCKS: u32 = 20480;

#[cfg(not(any(
    feature = "sweep-1mhz",
    feature = "sweep-4mhz",
    feature = "sweep-8mhz",
    feature = "sweep-12mhz",
    feature = "sweep-24mhz"
)))]
fn sweep_results_order_mhz() -> [(u8, &'static str); 5] {
    [
        (1, "1 MHz"),
        (4, "4 MHz"),
        (8, "8 MHz"),
        (12, "12 MHz"),
        (24, "24 MHz"),
    ]
}

#[used]
static mut SWEEP_RESULTS_BUF: [u8; 128] = [0; 128];

fn write_u8(buf: &mut [u8], pos: &mut usize, n: u8) {
    if n >= 100 {
        buf[*pos] = b'0' + (n / 100);
        *pos += 1;
    }
    if n >= 10 {
        buf[*pos] = b'0' + ((n / 10) % 10);
        *pos += 1;
    }
    buf[*pos] = b'0' + (n % 10);
    *pos += 1;
}

#[cfg(not(any(
    feature = "sweep-1mhz",
    feature = "sweep-4mhz",
    feature = "sweep-8mhz",
    feature = "sweep-12mhz",
    feature = "sweep-24mhz"
)))]
fn write_results_to_buf(results: &[(bool, u32); 5], best_mhz: u8) {
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(SWEEP_RESULTS_BUF) };
    let mut p = 0;
    let speeds = sweep_results_order_mhz();
    for (i, (id, _)) in speeds.iter().enumerate() {
        if p + 12 < buf.len() {
            write_u8(buf, &mut p, *id);
            buf[p] = b'=';
            p += 1;
            let (passed, _) = results[i];
            let s = if passed { b"PASS" } else { b"FAIL" };
            buf[p..p + s.len()].copy_from_slice(s);
            p += s.len();
            buf[p] = b',';
            p += 1;
        }
    }
    if p + 8 < buf.len() {
        buf[p..p + 4].copy_from_slice(b"REC=");
        p += 4;
        write_u8(buf, &mut p, best_mhz);
        buf[p] = 0;
    }
}

#[cfg(any(
    feature = "sweep-1mhz",
    feature = "sweep-4mhz",
    feature = "sweep-8mhz",
    feature = "sweep-12mhz",
    feature = "sweep-24mhz"
))]
fn write_single_freq_result(mhz: u8, passed: bool) {
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(SWEEP_RESULTS_BUF) };
    let mut p = 0;
    write_u8(buf, &mut p, mhz);
    buf[p] = b'=';
    p += 1;
    let s = if passed { b"PASS" } else { b"FAIL" };
    buf[p..p + s.len()].copy_from_slice(s);
    p += s.len();
    buf[p..p + 5].copy_from_slice(b",REC=");
    p += 5;
    write_u8(buf, &mut p, if passed { mhz } else { 1 });
    buf[p] = 0;
}

#[cfg(feature = "sweep-1mhz")]
const TARGET_MHZ: u8 = 1;
#[cfg(feature = "sweep-4mhz")]
const TARGET_MHZ: u8 = 4;
#[cfg(feature = "sweep-8mhz")]
const TARGET_MHZ: u8 = 8;
#[cfg(feature = "sweep-12mhz")]
const TARGET_MHZ: u8 = 12;
#[cfg(feature = "sweep-24mhz")]
const TARGET_MHZ: u8 = 24;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut cp = Peripherals::take().unwrap();

    let mut rcc = dp.RCC.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
    let mut delay = cp.SYST.delay(&rcc.clocks);

    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);
    let gpioe = dp.GPIOE.split(&mut rcc);
    let gpiof = dp.GPIOF.split(&mut rcc);
    let gpiog = dp.GPIOG.split(&mut rcc);
    let gpioh = dp.GPIOH.split(&mut rcc);
    let gpioi = dp.GPIOI.split(&mut rcc);

    defmt::info!("SDRAM init (for SDIO pin remainders)...");
    let (sdram_pins, remainders, _ph7) =
        split_sdram_pins(gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi);
    let _sdram = Sdram::new(dp.FMC, sdram_pins, &rcc.clocks, &mut delay);

    defmt::info!("SDIO init...");
    let (mut sdio, _pc1) = sdio::init(dp.SDIO, remainders, &mut rcc);

    defmt::info!("SD card init at 1 MHz...");
    if let Err(_e) = sdio::init_card(&mut sdio, &mut delay) {
        defmt::warn!("SD card init failed");
        defmt::info!("HIL_RESULT:sdio_speed_sweep:SKIP");
        defmt::info!("HIL_DETAIL:no_sd_card");
        loop {
            cortex_m::asm::wfi();
        }
    }

    let mut _dwt_ok = false;
    if DWT::has_cycle_counter() {
        cp.DCB.enable_trace();
        cp.DWT.enable_cycle_counter();
        _dwt_ok = true;
    }

    #[cfg(any(
        feature = "sweep-1mhz",
        feature = "sweep-4mhz",
        feature = "sweep-8mhz",
        feature = "sweep-12mhz",
        feature = "sweep-24mhz"
    ))]
    {
        defmt::info!(
            "Single-freq test: {} MHz requested, running at 400 kHz, {} blocks",
            TARGET_MHZ,
            SWEEP_BLOCKS
        );
        let start = if _dwt_ok {
            Some(DWT::cycle_count())
        } else {
            None
        };
        let (read, err) = sdio::test_raw_read(&mut sdio, SWEEP_BLOCKS);
        let passed = err == 0 && read == SWEEP_BLOCKS;
        if passed {
            if let Some(s) = start {
                let end = DWT::cycle_count();
                let cycles = end.wrapping_sub(s);
                const SYSCLK_HZ: u32 = 180_000_000;
                let secs = cycles as f32 / SYSCLK_HZ as f32;
                let mb_s = if secs > 0.0 {
                    (SWEEP_BLOCKS as f32 * 512.0) / 1_000_000.0 / secs
                } else {
                    0.0
                };
                let mb_s_int = mb_s as u32;
                let mb_s_centi = ((mb_s * 100.0) as u32) % 100;
                defmt::info!(
                    "  400 kHz: PASS ({} blocks, {}.{:02} MB/s)",
                    read,
                    mb_s_int,
                    mb_s_centi
                );
            } else {
                defmt::info!("  400 kHz: PASS ({} blocks)", read);
            }
        } else {
            defmt::warn!("  400 kHz: FAIL ({} read, {} errors)", read, err);
        }
        write_single_freq_result(TARGET_MHZ, passed);

        if passed {
            defmt::info!("HIL_RESULT:sdio_speed_sweep:PASS");
        } else {
            defmt::info!("HIL_RESULT:sdio_speed_sweep:FAIL");
            defmt::info!("HIL_DETAIL:{}_mhz_errors={}", TARGET_MHZ, err);
        }
        loop {
            cortex_m::asm::wfi();
        }
    }

    #[cfg(not(any(
        feature = "sweep-1mhz",
        feature = "sweep-4mhz",
        feature = "sweep-8mhz",
        feature = "sweep-12mhz",
        feature = "sweep-24mhz"
    )))]
    {
        let speeds = sweep_results_order_mhz();
        let mut results: [(bool, u32); 5] = [(false, 0); 5];

        defmt::info!("Speed sweep at 400 kHz: {} blocks per slot", SWEEP_BLOCKS);
        for (i, (_id, label)) in speeds.iter().map(|(a, b)| (*a, *b)).enumerate() {
            let (read, err) = sdio::test_raw_read(&mut sdio, SWEEP_BLOCKS);
            let passed = err == 0 && read == SWEEP_BLOCKS;
            results[i] = (passed, err);
            if passed {
                defmt::info!("  {}: PASS ({} blocks)", label, read);
            } else {
                defmt::warn!("  {}: FAIL ({} read, {} errors)", label, read, err);
            }
        }

        defmt::info!("=== SDIO SPEED SWEEP RESULTS (400 kHz) ===");
        for (i, (_id, label)) in speeds.iter().map(|(a, b)| (*a, *b)).enumerate() {
            let (passed, err) = results[i];
            if passed {
                defmt::info!("  {}: PASS", label);
            } else {
                defmt::info!("  {}: FAIL ({} errors)", label, err);
            }
        }
        let best_mhz = (0..5)
            .rev()
            .find(|&i| results[i].0)
            .map(|i| speeds[i].0)
            .unwrap_or(1);
        defmt::info!(
            "RECOMMENDATION: Use {} MHz for this card (when HAL exposes set_bus)",
            best_mhz
        );
        write_results_to_buf(&results, best_mhz);

        defmt::info!("Full read test at 400 kHz ({} blocks)...", FULL_TEST_BLOCKS);
        let (blocks_read, errors) = sdio::test_raw_read(&mut sdio, FULL_TEST_BLOCKS);

        defmt::info!(
            "SDIO speed sweep done: {} blocks read, {} errors",
            blocks_read,
            errors
        );

        let sweep_pass = results.iter().all(|(p, _)| *p);
        let full_pass = errors == 0 && blocks_read == FULL_TEST_BLOCKS;
        if sweep_pass && full_pass {
            defmt::info!("HIL_RESULT:sdio_speed_sweep:PASS");
        } else {
            defmt::info!("HIL_RESULT:sdio_speed_sweep:FAIL");
            if !sweep_pass {
                let fail_count = results.iter().filter(|(p, _)| !*p).count();
                defmt::info!("HIL_DETAIL:sweep_failures={}", fail_count);
            }
            if !full_pass {
                defmt::info!("HIL_DETAIL:full_test_errors={}", errors);
            }
        }

        loop {
            cortex_m::asm::wfi();
        }
    }
}
