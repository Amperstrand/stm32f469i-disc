#![deny(warnings)]
#![no_main]
#![no_std]

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::gpio::alt::fmc as alt;
use board::hal::ltdc::{Layer, PixelFormat};
use board::hal::{pac, prelude::*, rcc};
use board::lcd;
use board::sdram::{sdram_pins, Sdram};

const RED: u32 = 0xFFFF0000;
const GREEN: u32 = 0xFF00FF00;
const BLUE: u32 = 0xFF0000FF;
const WHITE: u32 = 0xFFFFFFFF;

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let cp = Peripherals::take().unwrap();

    let mut rcc = dp
        .RCC
        .freeze(rcc::Config::hse(8.MHz()).pclk2(32.MHz()).sysclk(180.MHz()));
    let mut delay = cp.SYST.delay(&rcc.clocks);

    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);
    let gpioe = dp.GPIOE.split(&mut rcc);
    let gpiof = dp.GPIOF.split(&mut rcc);
    let gpiog = dp.GPIOG.split(&mut rcc);
    let gpioh = dp.GPIOH.split(&mut rcc);
    let gpioi = dp.GPIOI.split(&mut rcc);

    let mut lcd_reset = gpioh.ph7.into_push_pull_output();
    lcd_reset.set_low();
    delay.delay_ms(20u32);
    lcd_reset.set_high();
    delay.delay_ms(10u32);

    defmt::info!("Initializing SDRAM...");
    let sdram = Sdram::new(
        dp.FMC,
        sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
        &rcc.clocks,
        &mut delay,
    );
    let orientation = lcd::DisplayOrientation::Portrait;
    let fb: &'static mut [u32] =
        unsafe { core::slice::from_raw_parts_mut(sdram.mem, orientation.fb_size()) };

    defmt::info!("Initializing ARGB8888 display...");
    let (mut display_ctrl, _controller, _orientation) = lcd::init_display_full_argb8888(
        dp.DSI,
        dp.LTDC,
        dp.DMA2D,
        &mut rcc,
        &mut delay,
        lcd::BoardHint::ForceNt35510,
        lcd::DisplayOrientation::Portrait,
    );
    display_ctrl.config_layer(Layer::L1, fb, PixelFormat::ARGB8888);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    let buf = display_ctrl
        .layer_buffer_mut(Layer::L1)
        .expect("layer L1 buffer");
    let width = orientation.width() as usize;

    for row in 0..orientation.height() as usize {
        let color = match row {
            0..200 => RED,
            200..400 => GREEN,
            400..600 => BLUE,
            _ => WHITE,
        };
        let start = row * width;
        let end = start + width;
        buf[start..end].fill(color);
    }
    display_ctrl.reload();

    defmt::info!("Display ready — ARGB8888 color bands rendered");
    defmt::info!("HIL_RESULT:display_test_rgb888:PASS");

    loop {
        cortex_m::asm::wfi();
    }
}
