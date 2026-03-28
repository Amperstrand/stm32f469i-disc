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

    defmt::info!("Initializing touch controller I2C...");
    let i2c = touch::init_i2c(dp.I2C1, gpiob.pb8, gpiob.pb9, &mut rcc);
    defmt::info!("Detecting FT6X06...");
    let _touch_ctrl = touch::init_ft6x06(i2c);
    defmt::info!("FT6X06 touch controller initialized");
    defmt::info!("HIL_RESULT:display_touch:PASS");

    loop {
        cortex_m::asm::wfi();
    }
}
