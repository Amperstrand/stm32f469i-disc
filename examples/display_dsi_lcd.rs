//! HIL test: DSI LCD init + one-frame render on the STM32F469I-DISCO display.
//!
//! Proves the full display pipeline: SDRAM init, DSI host, LCD controller
//! detection (NT35510/OTM8009A), LTDC config, and framebuffer write.
//! Renders one gradient frame then halts. Verify visually on the LCD panel.
//!
//! Run: `cargo run --release --example display_dsi_lcd`

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

fn hue_to_rgb565(hue: u32, level: u32) -> u16 {
    let hue = hue % 360;
    let sector = hue / 60;
    let fraction = hue % 60;
    let none = 0u32;
    let full = level;
    let rise = (level * fraction) / 60;
    let fall = (level * (60 - fraction)) / 60;
    let (r, g, b) = match sector {
        0 => (full, rise, none),
        1 => (fall, full, none),
        2 => (none, full, rise),
        3 => (none, fall, full),
        4 => (rise, none, full),
        5 => (full, none, fall),
        _ => (none, none, none),
    };
    let r5 = (r >> 3) as u16;
    let g6 = (g >> 2) as u16;
    let b5 = (b >> 3) as u16;
    (r5 << 11) | (g6 << 5) | b5
}

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
    let fb: &'static mut [u16] =
        unsafe { core::slice::from_raw_parts_mut(sdram.mem as *mut u16, orientation.fb_size()) };

    defmt::info!("Initializing display...");
    let (mut display_ctrl, _controller, _orientation) = lcd::init_display_full(
        dp.DSI,
        dp.LTDC,
        dp.DMA2D,
        &mut rcc,
        &mut delay,
        lcd::BoardHint::Unknown,
        lcd::DisplayOrientation::Portrait,
    );
    display_ctrl.config_layer(Layer::L1, fb, PixelFormat::RGB565);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    let buf = display_ctrl
        .layer_buffer_mut(Layer::L1)
        .expect("layer L1 buffer");
    let mut addr = 0;
    for row in 0..orientation.height() as u32 {
        let rgb = hue_to_rgb565(row * 360 / orientation.height() as u32, 255);
        for _col in 0..orientation.width() as u32 {
            buf[addr] = rgb;
            addr += 1;
        }
    }
    display_ctrl.reload();

    defmt::info!("Display ready — one frame rendered");
    defmt::info!("HIL_RESULT:display_dsi_lcd:PASS");

    loop {
        cortex_m::asm::wfi();
    }
}
