#![deny(warnings)]
#![allow(dead_code)]
#![no_main]
#![no_std]

use cortex_m_rt::entry;

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::gpio::alt::fmc as alt;
use board::hal::ltdc::{Layer, LtdcFramebuffer, PixelFormat};
use board::hal::pac::{CorePeripherals, Peripherals};
use board::hal::{prelude::*, rcc};
use board::lcd::{self, DisplayOrientation};
use board::sdram::{sdram_pins, Sdram};

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

const W: u16 = 480;
const H: u16 = 800;

#[entry]
fn main() -> ! {
    defmt::info!("ruler: RGB565 column ruler (CONTROL)");

    let dp = Peripherals::take().unwrap();
    let cp = CorePeripherals::take().unwrap();

    let mut rcc = dp
        .RCC
        .freeze(rcc::Config::hse(8.MHz()).pclk2(32.MHz()).sysclk(180.MHz()));
    let mut delay = cp.SYST.delay(&rcc.clocks);

    let _gpiob = dp.GPIOB.split(&mut rcc);
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

    let mut sdram = Sdram::new(
        dp.FMC,
        sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
        &rcc.clocks,
        &mut delay,
    );

    let orientation = DisplayOrientation::Portrait;

    let (mut display_ctrl, _controller, _orientation) = lcd::init_display_full(
        dp.DSI,
        dp.LTDC,
        dp.DMA2D,
        &mut rcc,
        &mut delay,
        lcd::BoardHint::ForceNt35510,
        orientation,
    );

    let buffer: &'static mut [u16] = sdram.subslice_mut(0, orientation.fb_size());
    let buffer_addr = buffer.as_mut_ptr() as usize;

    {
        let mut fb = LtdcFramebuffer::new(buffer, W, H);
        fb.clear(Rgb565::BLACK).ok();

        let white_t = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);
        let black_t = MonoTextStyle::new(&FONT_10X20, Rgb565::BLACK);

        let stripes: [(i32, u32, Rgb565, &str); 7] = [
            (0, 80, Rgb565::RED, "0"),
            (80, 80, Rgb565::GREEN, "80"),
            (160, 80, Rgb565::BLUE, "160"),
            (240, 80, Rgb565::YELLOW, "240"),
            (320, 80, Rgb565::CYAN, "320"),
            (400, 80, Rgb565::MAGENTA, "400"),
            (0, 1, Rgb565::WHITE, ""),
        ];

        for &(x, w, color, label) in &stripes {
            Rectangle::new(Point::new(x, 40), Size::new(w, (H - 40) as u32))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(&mut fb)
                .ok();
            if !label.is_empty() {
                let t = if color == Rgb565::BLACK {
                    white_t
                } else {
                    black_t
                };
                Text::new(label, Point::new(x + w as i32 / 2 - 10, 50), t)
                    .draw(&mut fb)
                    .ok();
            }
        }

        Line::new(Point::new(0, 39), Point::new(W as i32 - 1, 39))
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 2))
            .draw(&mut fb)
            .ok();

        Text::new("RGB565 CONTROL", Point::new(160, 10), white_t)
            .draw(&mut fb)
            .ok();
    }

    let buffer: &'static mut [u16] = unsafe {
        &mut *core::ptr::slice_from_raw_parts_mut(buffer_addr as *mut u16, orientation.fb_size())
    };
    display_ctrl.config_layer(Layer::L1, buffer, PixelFormat::RGB565);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    loop {
        cortex_m::asm::wfi();
    }
}
