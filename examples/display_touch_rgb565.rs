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
use board::lcd;
use board::sdram::{sdram_pins, Sdram};
use board::touch::{self, FT6X06_I2C_ADDR};

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 800;
const BAND_HEIGHT: i32 = 200;
const TITLE_Y: i32 = 24;
const LABEL_Y: i32 = 48;
const POLL_MS: u32 = 20;
const CLEAR_AFTER_MS: u32 = 2000;

fn band_color(y: u16) -> Rgb565 {
    match y {
        0..=199 => Rgb565::RED,
        200..=399 => Rgb565::GREEN,
        400..=599 => Rgb565::BLUE,
        _ => Rgb565::WHITE,
    }
}

fn draw_color_bands(fb: &mut LtdcFramebuffer<u16>) {
    for (idx, color) in [Rgb565::RED, Rgb565::GREEN, Rgb565::BLUE, Rgb565::WHITE]
        .into_iter()
        .enumerate()
    {
        Rectangle::new(
            Point::new(0, (idx as i32) * BAND_HEIGHT),
            Size::new(WIDTH, BAND_HEIGHT as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(fb)
        .ok();
    }
}

fn draw_labels(fb: &mut LtdcFramebuffer<u16>) {
    Rectangle::new(Point::new(0, 0), Size::new(WIDTH, 72))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(fb)
        .ok();

    let text_style = MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE);

    Text::new("RGB565 + Touch", Point::new(170, TITLE_Y), text_style)
        .draw(fb)
        .ok();
    Text::new("u16 / 2bpp / 480x800", Point::new(140, LABEL_Y), text_style)
        .draw(fb)
        .ok();
}

fn draw_crosshair(fb: &mut LtdcFramebuffer<u16>, x: u16, y: u16, color: Rgb565) {
    let style = PrimitiveStyle::with_stroke(color, 1);

    Line::new(
        Point::new(0, y as i32),
        Point::new(WIDTH as i32 - 1, y as i32),
    )
    .into_styled(style)
    .draw(fb)
    .ok();
    Line::new(
        Point::new(x as i32, 0),
        Point::new(x as i32, HEIGHT as i32 - 1),
    )
    .into_styled(style)
    .draw(fb)
    .ok();
    // Full-height vertical line
    Line::new(
        Point::new(x as i32, 0),
        Point::new(x as i32, HEIGHT as i32 - 1),
    )
    .into_styled(style)
    .draw(fb)
    .ok();
}

fn restore_crosshair(fb: &mut LtdcFramebuffer<u16>, x: u16, y: u16) {
    draw_crosshair(fb, x, y, band_color(y));

    if y <= 72 {
        draw_labels(fb);
    }
}

#[entry]
fn main() -> ! {
    defmt::info!("display_touch_rgb565: init...");

    let dp = Peripherals::take().unwrap();
    let cp = CorePeripherals::take().unwrap();

    defmt::info!("display_touch_rgb565: freeze rcc");
    let mut rcc = dp
        .RCC
        .freeze(rcc::Config::hse(8.MHz()).pclk2(32.MHz()).sysclk(180.MHz()));
    let mut delay = cp.SYST.delay(&rcc.clocks);

    defmt::info!("display_touch_rgb565: split gpio");
    let gpiob = dp.GPIOB.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);
    let gpioe = dp.GPIOE.split(&mut rcc);
    let gpiof = dp.GPIOF.split(&mut rcc);
    let gpiog = dp.GPIOG.split(&mut rcc);
    let gpioh = dp.GPIOH.split(&mut rcc);
    let gpioi = dp.GPIOI.split(&mut rcc);

    defmt::info!("display_touch_rgb565: toggle lcd reset");
    let mut lcd_reset = gpioh.ph7.into_push_pull_output();
    lcd_reset.set_low();
    delay.delay_ms(20u32);
    lcd_reset.set_high();
    delay.delay_ms(10u32);

    defmt::info!("display_touch_rgb565: init sdram");
    let mut sdram = Sdram::new(
        dp.FMC,
        sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
        &rcc.clocks,
        &mut delay,
    );

    defmt::info!("display_touch_rgb565: init display");
    let orientation = lcd::DisplayOrientation::Portrait;
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
    let mut fb = LtdcFramebuffer::new(buffer, orientation.width(), orientation.height());
    fb.clear(Rgb565::BLACK).ok();

    defmt::info!("display_touch_rgb565: render diagnostics");
    draw_color_bands(&mut fb);
    draw_labels(&mut fb);

    let buffer = fb.into_inner();
    let buffer_addr = buffer.as_mut_ptr() as usize;

    display_ctrl.config_layer(Layer::L1, buffer, PixelFormat::RGB565);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    let buffer: &'static mut [u16] = unsafe {
        &mut *core::ptr::slice_from_raw_parts_mut(buffer_addr as *mut u16, orientation.fb_size())
    };
    let mut fb = LtdcFramebuffer::new(buffer, orientation.width(), orientation.height());

    defmt::info!("display_touch_rgb565: init touch");
    let i2c = touch::init_i2c(dp.I2C1, gpiob.pb8, gpiob.pb9, &mut rcc);
    let touch = touch::init_ft6x06(i2c);
    let mut i2c = touch.destroy();

    defmt::info!("display_touch_rgb565: touch loop");
    let mut last_crosshair: Option<(u16, u16)> = None;
    let mut idle_ms = CLEAR_AFTER_MS;

    loop {
        let mut status_buf = [0u8; 1];
        match i2c.write_read(FT6X06_I2C_ADDR, &[0x02], &mut status_buf) {
            Ok(()) if status_buf[0] > 0 => {
                let mut touch_buf = [0u8; 4];
                match i2c.write_read(FT6X06_I2C_ADDR, &[0x03], &mut touch_buf) {
                    Ok(()) => {
                        let x = ((touch_buf[0] & 0x0F) as u16) << 8 | touch_buf[1] as u16;
                        let y = ((touch_buf[2] & 0x0F) as u16) << 8 | touch_buf[3] as u16;

                        if !(3..=476).contains(&x) || !(3..=796).contains(&y) {
                            delay.delay_ms(POLL_MS);
                            idle_ms = idle_ms.saturating_add(POLL_MS).min(CLEAR_AFTER_MS);
                            continue;
                        }

                        if let Some((prev_x, prev_y)) = last_crosshair {
                            restore_crosshair(&mut fb, prev_x, prev_y);
                        }

                        draw_crosshair(&mut fb, x, y, Rgb565::YELLOW);
                        defmt::info!("Touch: x={}, y={}", x, y);
                        last_crosshair = Some((x, y));
                        idle_ms = 0;
                    }
                    Err(_) => {
                        defmt::warn!("Touch read error");
                    }
                }
            }
            Ok(()) => {
                if let Some((x, y)) = last_crosshair {
                    idle_ms = idle_ms.saturating_add(POLL_MS).min(CLEAR_AFTER_MS);
                    if idle_ms >= CLEAR_AFTER_MS {
                        restore_crosshair(&mut fb, x, y);
                        last_crosshair = None;
                    }
                }
            }
            Err(_) => {
                defmt::warn!("Touch read error");
            }
        }

        delay.delay_ms(POLL_MS);
    }
}
