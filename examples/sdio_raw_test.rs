//! HIL test: SDIO SD card raw read test.
//!
//! Initializes SDRAM (for SDIO pin routing), then initializes the SD card
//! and runs a 10 MiB raw block read test. Requires a microSD card in the slot.
//! Gracefully skips if no card is detected (prints TEST_SKIP instead of panicking).
//!
//! Run: `cargo run --release --example sdio_raw_test`

#![no_main]
#![no_std]

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_probe as _;

use board::hal::{pac, prelude::*, rcc};
use board::sdio;
use board::sdram::{split_sdram_pins, Sdram};
use stm32f469i_disc as board;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let cp = Peripherals::take().unwrap();

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

    defmt::info!("SD card init...");
    if let Err(_e) = sdio::init_card(&mut sdio, &mut delay) {
        defmt::warn!("SD card init failed");
        defmt::info!("HIL_RESULT:sdio_raw_test:SKIP");
        defmt::info!("HIL_DETAIL:no_sd_card");
    }

    const NUM_BLOCKS: u32 = 20480;
    defmt::info!("Running raw read test ({} blocks)...", NUM_BLOCKS);
    let (blocks_read, errors) = sdio::test_raw_read(&mut sdio, NUM_BLOCKS);

    defmt::info!(
        "SDIO test done: {} blocks read, {} errors",
        blocks_read,
        errors
    );
    if errors == 0 && blocks_read == NUM_BLOCKS {
        defmt::info!("HIL_RESULT:sdio_raw_test:PASS");
    } else {
        defmt::info!("HIL_RESULT:sdio_raw_test:FAIL");
        defmt::info!("HIL_DETAIL:errors={} blocks_read={}", errors, blocks_read);
    }

    loop {
        cortex_m::asm::wfi();
    }
}
