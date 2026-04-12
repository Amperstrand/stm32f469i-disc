#![deny(warnings)]
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
use board::touch::{self, FT6X06_I2C_ADDR};

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 800;
const BAND_HEIGHT: i32 = 200;
const TOUCH_POLL_MS: u32 = 50;
const CROSSHAIR_TIMEOUT_MS: u32 = 2000;
const TITLE: &str = "ARGB8888 + Touch";
const LABEL: &str = "u32 / 4bpp / 480x800";

fn band_color(y: i32) -> Rgb888 {
    match y {
        ..200 => Rgb888::RED,
        200..400 => Rgb888::GREEN,
        400..600 => Rgb888::BLUE,
        _ => Rgb888::WHITE,
    }
}

fn centered_x(text: &str) -> i32 {
    ((WIDTH as i32) - (text.len() as i32 * 10)) / 2
}

fn draw_static_scene(fb: &mut FramebufferView<'_>) {
    for (index, color) in [Rgb888::RED, Rgb888::GREEN, Rgb888::BLUE, Rgb888::WHITE]
        .into_iter()
        .enumerate()
    {
        Rectangle::new(
            Point::new(0, (index as i32) * BAND_HEIGHT),
            Size::new(WIDTH, BAND_HEIGHT as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(fb)
        .ok();
    }

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);

    Rectangle::new(Point::new(110, 8), Size::new(260, 56))
        .into_styled(PrimitiveStyle::with_fill(Rgb888::BLACK))
        .draw(fb)
        .ok();

    Text::new(TITLE, Point::new(centered_x(TITLE), 28), text_style)
        .draw(fb)
        .ok();
    Text::new(LABEL, Point::new(centered_x(LABEL), 52), text_style)
        .draw(fb)
        .ok();
}

fn draw_crosshair(fb: &mut FramebufferView<'_>, x: i32, y: i32, color: Rgb888) {
    let style = PrimitiveStyle::with_stroke(color, 1);

    Line::new(Point::new(0, y), Point::new(WIDTH as i32 - 1, y))
        .into_styled(style)
        .draw(fb)
        .ok();

    Line::new(Point::new(x, 0), Point::new(x, HEIGHT as i32 - 1))
        .into_styled(style)
        .draw(fb)
        .ok();
}

fn erase_crosshair(fb: &mut FramebufferView<'_>, x: i32, y: i32) {
    Line::new(Point::new(0, y), Point::new(WIDTH as i32 - 1, y))
        .into_styled(PrimitiveStyle::with_stroke(band_color(y), 1))
        .draw(fb)
        .ok();

    let top = 0;
    let bottom = HEIGHT as i32 - 1;
    for row in top..=bottom {
        Rectangle::new(Point::new(x, row), Size::new(1, 1))
            .into_styled(PrimitiveStyle::with_fill(band_color(row)))
            .draw(fb)
            .ok();
    }

    Rectangle::new(Point::new(110, 8), Size::new(260, 56))
        .into_styled(PrimitiveStyle::with_fill(Rgb888::BLACK))
        .draw(fb)
        .ok();

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);
    Text::new(TITLE, Point::new(centered_x(TITLE), 28), text_style)
        .draw(fb)
        .ok();
    Text::new(LABEL, Point::new(centered_x(LABEL), 52), text_style)
        .draw(fb)
        .ok();
}

#[entry]
fn main() -> ! {
    defmt::info!("display_touch_argb8888: init...");

    let dp = Peripherals::take().unwrap();
    let cp = CorePeripherals::take().unwrap();

    let mut rcc = dp
        .RCC
        .freeze(rcc::Config::hse(8.MHz()).pclk2(32.MHz()).sysclk(180.MHz()));
    let mut delay = cp.SYST.delay(&rcc.clocks);

    let gpiob = dp.GPIOB.split(&mut rcc);
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
        let mut fb = FramebufferView::new(
            buffer,
            orientation.width() as u32,
            orientation.height() as u32,
        );
        fb.clear(Rgb888::BLACK);
        draw_static_scene(&mut fb);
    }

    let buffer: &'static mut [u32] = unsafe {
        &mut *core::ptr::slice_from_raw_parts_mut(buffer_addr as *mut u32, orientation.fb_size())
    };
    display_ctrl.config_layer(Layer::L1, buffer, PixelFormat::ARGB8888);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    let buffer: &'static mut [u32] = unsafe {
        &mut *core::ptr::slice_from_raw_parts_mut(buffer_addr as *mut u32, orientation.fb_size())
    };

    let mut fb = FramebufferView::new(
        buffer,
        orientation.width() as u32,
        orientation.height() as u32,
    );
    let i2c = touch::init_i2c(dp.I2C1, gpiob.pb8, gpiob.pb9, &mut rcc);
    let touch = touch::init_ft6x06(i2c);
    let mut i2c = touch.destroy();
    let mut last_crosshair: Option<(i32, i32)> = None;
    let mut idle_ms = 0u32;

    loop {
        let mut status_buf = [0u8; 1];
        match i2c.write_read(FT6X06_I2C_ADDR, &[0x02], &mut status_buf) {
            Ok(()) if status_buf[0] > 0 => {
                let mut touch_buf = [0u8; 4];
                match i2c.write_read(FT6X06_I2C_ADDR, &[0x03], &mut touch_buf) {
                    Ok(()) => {
                        let x = (((touch_buf[0] & 0x0F) as u16) << 8) | touch_buf[1] as u16;
                        let y = (((touch_buf[2] & 0x0F) as u16) << 8) | touch_buf[3] as u16;

                        if (3..=476).contains(&x) && (3..=796).contains(&y) {
                            let x = x as i32;
                            let y = y as i32;

                            if let Some((old_x, old_y)) = last_crosshair.take() {
                                erase_crosshair(&mut fb, old_x, old_y);
                            }

                            draw_crosshair(&mut fb, x, y, Rgb888::CSS_YELLOW);
                            defmt::info!("Touch: x={}, y={}", x, y);
                            last_crosshair = Some((x, y));
                            idle_ms = 0;
                        }
                    }
                    Err(_) => defmt::warn!("Touch read error"),
                }
            }
            Ok(_) => {
                idle_ms = idle_ms.saturating_add(TOUCH_POLL_MS);
                if idle_ms >= CROSSHAIR_TIMEOUT_MS {
                    if let Some((x, y)) = last_crosshair.take() {
                        erase_crosshair(&mut fb, x, y);
                    }
                    idle_ms = 0;
                }
            }
            Err(_) => defmt::warn!("Touch read error"),
        }

        delay.delay_ms(TOUCH_POLL_MS);
    }
}
