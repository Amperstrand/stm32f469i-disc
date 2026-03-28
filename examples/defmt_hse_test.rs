#![no_main]
#![no_std]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_probe as _;
use stm32f469i_disc as board;

use board::hal::prelude::*;

#[entry]
fn main() -> ! {
    defmt::info!("HIL_RESULT:defmt_hse_test:START");

    let pac = unsafe { board::hal::pac::Peripherals::steal() };
    let rcc = pac.RCC.constrain();
    let rcc = rcc.freeze(board::hal::rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
    let clocks = rcc.clocks;

    defmt::info!("HIL_RESULT:defmt_hse_test:PASS");
    defmt::info!("HIL_DETAIL:sysclk={} Hz", clocks.sysclk().raw());

    loop {
        cortex_m::asm::wfi();
    }
}
