#![deny(warnings)]
#![allow(dead_code)]
#![no_main]
#![no_std]

use cortex_m_rt::entry;

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::gpio::alt::fmc as alt;
use board::hal::ltdc::{Layer, PixelFormat};
use board::hal::pac::{CorePeripherals, Peripherals};
use board::hal::{prelude::*, rcc};
use board::lcd::{self, DisplayOrientation, FramebufferView};
use board::sdram::{sdram_pins, Sdram};

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

const W: u32 = 480;
const H: u32 = 800;

#[entry]
fn main() -> ! {
    defmt::info!("ruler: ARGB8888 column ruler");

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

    let (mut display_ctrl, _controller, _orientation) = lcd::init_display_full_argb8888(
        dp.DSI,
        dp.LTDC,
        dp.DMA2D,
        &mut rcc,
        &mut delay,
        lcd::BoardHint::ForceNt35510,
        orientation,
    );

    let buffer: &'static mut [u32] = sdram.subslice_mut(0, orientation.fb_size());
    let buffer_addr = buffer.as_mut_ptr() as usize;

    {
        let mut fb = FramebufferView::new(buffer, W, H);
        fb.clear(Rgb888::BLACK);

        let white_t = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);
        let black_t = MonoTextStyle::new(&FONT_10X20, Rgb888::BLACK);

        // Color stripes for horizontal shift measurement.
        let stripes: [(i32, u32, Rgb888, &str); 7] = [
            (0, 80, Rgb888::RED, "0"),
            (80, 80, Rgb888::GREEN, "80"),
            (160, 80, Rgb888::BLUE, "160"),
            (240, 80, Rgb888::YELLOW, "240"),
            (320, 80, Rgb888::CYAN, "320"),
            (400, 80, Rgb888::MAGENTA, "400"),
            (0, 1, Rgb888::WHITE, ""),
        ];

        for &(x, w, color, label) in &stripes {
            Rectangle::new(Point::new(x, 40), Size::new(w, H - 40))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(&mut fb)
                .ok();
            if !label.is_empty() {
                let t = if color == Rgb888::BLACK {
                    white_t
                } else {
                    black_t
                };
                Text::new(label, Point::new(x + w as i32 / 2 - 10, 50), t)
                    .draw(&mut fb)
                    .ok();
            }
        }

        // Rainbow gradient edge at rightmost 10px (x=470..479) — distinct 1px strips
        // let you count exactly which pixels are visible vs pushed off-screen.
        let edge_colors: [(i32, Rgb888, &str); 10] = [
            (470, Rgb888::new(255, 0, 0), "R"),
            (471, Rgb888::new(0, 255, 0), "G"),
            (472, Rgb888::new(0, 0, 255), "B"),
            (473, Rgb888::new(255, 255, 0), "Y"),
            (474, Rgb888::new(255, 0, 255), "M"),
            (475, Rgb888::new(0, 255, 255), "C"),
            (476, Rgb888::new(255, 128, 0), "O"),
            (477, Rgb888::new(128, 0, 255), "P"),
            (478, Rgb888::new(255, 255, 255), "W"),
            (479, Rgb888::new(128, 128, 128), "X"),
        ];
        for &(x, color, label) in &edge_colors {
            Rectangle::new(Point::new(x, 40), Size::new(1, H - 40))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(&mut fb)
                .ok();
            Text::new(label, Point::new(x - 3, 50), white_t)
                .draw(&mut fb)
                .ok();
        }

        Line::new(Point::new(0, 39), Point::new(W as i32 - 1, 39))
            .into_styled(PrimitiveStyle::with_stroke(Rgb888::WHITE, 2))
            .draw(&mut fb)
            .ok();

        // Alternating white/red 1px border on all 4 edges — white at outer edge,
        // red at 1px inset. Any cropping is immediately visible.
        let one_px = PrimitiveStyle::with_fill(Rgb888::WHITE);
        let one_px_r = PrimitiveStyle::with_fill(Rgb888::RED);
        for y in 0..H as i32 {
            Rectangle::new(Point::new(0, y), Size::new(1, 1))
                .into_styled(one_px)
                .draw(&mut fb)
                .ok();
            Rectangle::new(Point::new(1, y), Size::new(1, 1))
                .into_styled(one_px_r)
                .draw(&mut fb)
                .ok();
            Rectangle::new(Point::new(478, y), Size::new(1, 1))
                .into_styled(one_px_r)
                .draw(&mut fb)
                .ok();
            Rectangle::new(Point::new(479, y), Size::new(1, 1))
                .into_styled(one_px)
                .draw(&mut fb)
                .ok();
        }
        for x in 0..W as i32 {
            Rectangle::new(Point::new(x, 0), Size::new(1, 1))
                .into_styled(one_px)
                .draw(&mut fb)
                .ok();
            Rectangle::new(Point::new(x, 1), Size::new(1, 1))
                .into_styled(one_px_r)
                .draw(&mut fb)
                .ok();
            Rectangle::new(Point::new(x, 798), Size::new(1, 1))
                .into_styled(one_px_r)
                .draw(&mut fb)
                .ok();
            Rectangle::new(Point::new(x, 799), Size::new(1, 1))
                .into_styled(one_px)
                .draw(&mut fb)
                .ok();
        }

        Text::new("ARGB8888 BORDER TEST", Point::new(120, 10), white_t)
            .draw(&mut fb)
            .ok();

        for y in (0..H as i32).step_by(80) {
            Line::new(Point::new(0, y), Point::new(W as i32 - 1, y))
                .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(64, 64, 64), 1))
                .draw(&mut fb)
                .ok();
        }
    }

    let buffer: &'static mut [u32] = unsafe {
        &mut *core::ptr::slice_from_raw_parts_mut(buffer_addr as *mut u32, orientation.fb_size())
    };
    display_ctrl.config_layer(Layer::L1, buffer, PixelFormat::ARGB8888);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    loop {
        cortex_m::asm::wfi();
    }
}
