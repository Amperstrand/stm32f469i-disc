//! HIL test: FT6X06 touch controller I2C detection.
//!
//! Proves I2C bus init and FT6X06 touch controller communication.
//! Does NOT test swipe gestures (requires human interaction).
//! Skips if no touch controller is found (e.g., B07 board with OTM8009A).
//!
//! Run: `cargo run --release --example display_touch`

#![deny(warnings)]
#![no_main]
#![no_std]

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::{pac, prelude::*, rcc};
use board::touch;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let _cp = Peripherals::take().unwrap();

    let mut rcc = dp.RCC.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));

    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);

    defmt::info!("Initializing touch controller I2C...");
    let i2c = touch::init_i2c(dp.I2C1, gpiob.pb8, gpiob.pb9, &mut rcc);
    let ts_int = gpioc.pc1.into_pull_down_input();

    defmt::info!("Detecting FT6X06...");
    let touch_ctrl = touch::init_ft6x06(&i2c, ts_int);
    if touch_ctrl.is_some() {
        defmt::info!("FT6X06 touch controller detected");
        defmt::info!("HIL_RESULT:display_touch:PASS");
    } else {
        defmt::warn!("FT6X06 touch controller not detected");
        defmt::info!("HIL_RESULT:display_touch:SKIP");
        defmt::info!("HIL_DETAIL:no_touch_controller_found");
    }

    loop {
        cortex_m::asm::wfi();
    }
}
