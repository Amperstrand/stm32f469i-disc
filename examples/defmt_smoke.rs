#![no_main]
#![no_std]

use cortex_m_rt::entry;
use defmt_rtt as _;
use panic_probe as _;
use stm32f469i_disc as _;

#[entry]
fn main() -> ! {
    defmt::info!("HIL_RESULT:defmt_smoke:PASS");
    defmt::info!("HIL_DETAIL:hello from defmt");
    defmt::flush();
    loop {
        cortex_m::asm::wfi();
    }
}
