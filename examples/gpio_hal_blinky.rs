//! HIL test: GPIO LED cycle on the STM32F469I-DISCO board.
//!
//! Probes GPIO initialization and LED toggle. Runs one full on/off cycle
//! for all 4 LEDs, then prints HIL_RESULT in a loop until power-cycled.
//!
//! Run: `timeout 15 probe-rs run --chip STM32F469NIHx target/.../gpio_hal_blinky`

#![no_main]
#![no_std]

use panic_probe as _;

use defmt_rtt as _;

use stm32f469i_disc as board;

use crate::board::{
    hal::{pac, prelude::*, rcc},
    led::Leds,
};

use cortex_m::peripheral::Peripherals;

use cortex_m_rt::entry;

#[entry]
fn main() -> ! {
    if let (Some(p), Some(cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let gpiod = p.GPIOD.split(&mut rcc);
        let gpiog = p.GPIOG.split(&mut rcc);
        let gpiok = p.GPIOK.split(&mut rcc);
        let mut delay = cp.SYST.delay(&clocks);
        let mut leds = Leds::new(gpiod, gpiog, gpiok);

        for _ in 0..3 {
            for led in leds.iter_mut() {
                led.on();
            }
            delay.delay_ms(500u32);
            for led in leds.iter_mut() {
                led.off();
            }
            delay.delay_ms(500u32);
        }

        defmt::info!("HIL_RESULT:gpio_hal_blinky:PASS");
    } else {
        defmt::info!("HIL_RESULT:gpio_hal_blinky:FAIL");
        defmt::info!("HIL_DETAIL:peripherals_take_failed");
    }

    loop {
        cortex_m::asm::wfi();
    }
}
