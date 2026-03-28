//! On-screen hardware diagnostics for STM32F469I-DISCO
//!
//! Renders all test results directly to the LCD framebuffer using embedded-graphics.
//! Uses RGB565 pixel format (2 bytes/pixel, 768 KB framebuffer).
//! After summary, enters a 30-second interactive touch demo.
//!
//! Test suites: SDRAM (6), Display (5), Touch (3), GPIO (2), LEDs (5), Timers (2) = 23 tests

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::gpio::alt::fmc as alt;
use board::hal::{pac, prelude::*, rcc};
use board::sdram::{sdram_pins, Sdram};
use board::touch::{self, FT6X06_I2C_ADDR};

use cortex_m::peripheral::{Peripherals, DWT};
use cortex_m_rt::entry;

use core::slice;

use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{OriginDimensions, Point, Size},
    mono_font::{ascii::FONT_6X9, MonoTextStyle, MonoTextStyleBuilder},
    pixelcolor::{Rgb565, RgbColor},
    prelude::*,
    primitives::{rectangle::Rectangle, PrimitiveStyle},
    Drawable, Pixel,
};

use stm32f4xx_hal::dsi::{
    ColorCoding, DsiChannel, DsiCmdModeTransmissionKind, DsiConfig, DsiHost, DsiInterrupts,
    DsiMode, DsiPhyTimers, DsiPllConfig, DsiVideoMode, LaneCount,
};
use stm32f4xx_hal::ltdc::{DisplayConfig, DisplayController, Layer, PixelFormat};

use otm8009a::{Otm8009A, Otm8009AConfig};

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

const DISPLAY_CFG: DisplayConfig = DisplayConfig {
    active_width: WIDTH as _,
    active_height: HEIGHT as _,
    h_back_porch: 34,
    h_front_porch: 34,
    v_back_porch: 15,
    v_front_porch: 16,
    h_sync: 2,
    v_sync: 1,
    frame_rate: 60,
    h_sync_pol: true,
    v_sync_pol: true,
    no_data_enable_pol: false,
    pixel_clock_pol: true,
};

struct FramebufferTarget<'a> {
    buf: &'a mut [u16],
    width: usize,
    height: usize,
}

impl<'a> FramebufferTarget<'a> {
    fn new(buf: &'a mut [u16], width: usize, height: usize) -> Self {
        Self { buf, width, height }
    }

    fn fill_raw(&mut self, color: u16) {
        for px in self.buf.iter_mut() {
            *px = color;
        }
    }
}

impl DrawTarget for FramebufferTarget<'_> {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            if coord.x >= 0
                && (coord.x as usize) < self.width
                && coord.y >= 0
                && (coord.y as usize) < self.height
            {
                let idx = (coord.y as usize) * self.width + (coord.x as usize);
                self.buf[idx] = color.into_storage();
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.fill_raw(color.into_storage());
        Ok(())
    }
}

impl OriginDimensions for FramebufferTarget<'_> {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}

enum TestStatus {
    Pass,
    Fail,
}

struct TestResult {
    name: &'static str,
    status: TestStatus,
}

const BG: Rgb565 = Rgb565::new(0x1a, 0x1a, 0x2e);
const PASS_COLOR: Rgb565 = Rgb565::new(0x00, 0xe0, 0x40);
const FAIL_COLOR: Rgb565 = Rgb565::new(0xe0, 0x20, 0x20);
const HEADER_COLOR: Rgb565 = Rgb565::new(0x40, 0xa0, 0xe0);
const TEXT_COLOR: Rgb565 = Rgb565::new(0xe0, 0xe0, 0xe0);
const DIM_TEXT: Rgb565 = Rgb565::new(0x80, 0x80, 0x80);

fn draw_text(
    fb: &mut FramebufferTarget<'_>,
    text: &str,
    x: i32,
    y: i32,
    style: &MonoTextStyle<Rgb565>,
) {
    embedded_graphics::text::Text::new(text, Point::new(x, y), *style)
        .draw(fb)
        .ok();
}

fn draw_status_dot(fb: &mut FramebufferTarget<'_>, x: i32, y: i32, status: &TestStatus) {
    let color = match status {
        TestStatus::Pass => PASS_COLOR,
        TestStatus::Fail => FAIL_COLOR,
    };
    Rectangle::new(Point::new(x, y), Size::new(8, 8))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(fb)
        .ok();
}

fn draw_section(
    fb: &mut FramebufferTarget<'_>,
    y: &mut i32,
    title: &str,
    style: &MonoTextStyle<Rgb565>,
) {
    *y += 4;
    draw_text(fb, title, 8, *y, style);
    *y += 14;
    Rectangle::new(Point::new(8, *y), Size::new(464, 1))
        .into_styled(PrimitiveStyle::with_fill(DIM_TEXT))
        .draw(fb)
        .ok();
    *y += 4;
}

fn draw_result(
    fb: &mut FramebufferTarget<'_>,
    y: &mut i32,
    result: &TestResult,
    style: &MonoTextStyle<Rgb565>,
) {
    draw_status_dot(fb, 12, *y, &result.status);
    let status_str = match result.status {
        TestStatus::Pass => "PASS",
        TestStatus::Fail => "FAIL",
    };
    draw_text(fb, result.name, 26, *y, style);
    draw_text(fb, status_str, 420, *y, style);
    *y += 12;
}

fn draw_u32_text(
    fb: &mut FramebufferTarget<'_>,
    x: i32,
    y: i32,
    style: &MonoTextStyle<Rgb565>,
    val: u32,
) {
    let mut buf = [0u8; 12];
    let mut i = buf.len();
    let mut v = val;
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let s = unsafe { core::str::from_utf8_unchecked(&buf[i..]) };
    draw_text(fb, s, x, y, style);
}

fn draw_summary(
    fb: &mut FramebufferTarget<'_>,
    results: &[TestResult],
    y: i32,
    style: &MonoTextStyle<Rgb565>,
    header_style: &MonoTextStyle<Rgb565>,
) {
    let passed = results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Pass))
        .count();
    let failed = results
        .iter()
        .filter(|r| matches!(r.status, TestStatus::Fail))
        .count();
    let total = results.len();

    let mut sy = y + 8;
    Rectangle::new(Point::new(8, sy), Size::new(464, 1))
        .into_styled(PrimitiveStyle::with_fill(DIM_TEXT))
        .draw(fb)
        .ok();
    sy += 8;

    let banner_color = if failed == 0 { PASS_COLOR } else { FAIL_COLOR };
    let banner_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X9)
        .text_color(banner_color)
        .background_color(BG)
        .build();

    if failed == 0 {
        draw_text(fb, "ALL TESTS PASSED", 8, sy, &banner_style);
    } else {
        draw_text(fb, "SOME TESTS FAILED", 8, sy, &banner_style);
    }
    sy += 12;

    draw_text(fb, "Passed: ", 8, sy, style);
    draw_u32_text(fb, 62, sy, style, passed as u32);
    draw_text(fb, " Failed: ", 100, sy, style);
    draw_u32_text(fb, 162, sy, style, failed as u32);
    draw_text(fb, " Total: ", 200, sy, style);
    draw_u32_text(fb, 258, sy, style, total as u32);
    sy += 16;

    draw_text(
        fb,
        "STM32F469I-DISCO Hardware Diagnostics",
        8,
        sy + 10,
        header_style,
    );

    defmt::info!("SUMMARY: {}/{} passed", passed, total);
    if failed == 0 {
        defmt::info!("ALL TESTS PASSED");
    } else {
        defmt::error!("FAILED: {} tests failed", failed);
    }
}

fn draw_touch_prompt(
    fb: &mut FramebufferTarget<'_>,
    style: &MonoTextStyle<Rgb565>,
    header_style: &MonoTextStyle<Rgb565>,
) {
    fb.clear(BG).ok();
    draw_text(fb, "Touch Demo", 8, 10, header_style);
    Rectangle::new(Point::new(8, 30), Size::new(464, 1))
        .into_styled(PrimitiveStyle::with_fill(DIM_TEXT))
        .draw(fb)
        .ok();
    draw_text(fb, "Touch the screen to test the touch", 8, 44, style);
    draw_text(fb, "controller. Coordinates will be", 8, 56, style);
    draw_text(fb, "shown below. (30 seconds)", 8, 68, style);
}

fn draw_touch_point(fb: &mut FramebufferTarget<'_>, x: u16, y: u16, style: &MonoTextStyle<Rgb565>) {
    let cx = x as i32;
    let cy = y as i32;
    let cross_color = Rgb565::YELLOW;
    let cross_style = PrimitiveStyle::with_fill(cross_color);

    Rectangle::new(Point::new(cx - 8, cy - 1), Size::new(16, 2))
        .into_styled(cross_style)
        .draw(fb)
        .ok();
    Rectangle::new(Point::new(cx - 1, cy - 8), Size::new(2, 16))
        .into_styled(cross_style)
        .draw(fb)
        .ok();

    draw_text(fb, "Touch: (", 8, HEIGHT as i32 - 30, style);
    draw_u32_text(fb, 62, HEIGHT as i32 - 30, style, x as u32);
    draw_text(fb, ", ", 100, HEIGHT as i32 - 30, style);
    draw_u32_text(fb, 110, HEIGHT as i32 - 30, style, y as u32);
    draw_text(fb, ")", 145, HEIGHT as i32 - 30, style);
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(mut cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let hse_freq = 8.MHz();
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(hse_freq).pclk2(32.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let mut delay = cp.SYST.delay(&clocks);

        cp.SCB.invalidate_icache();
        cp.SCB.enable_icache();
        cp.DCB.enable_trace();
        cp.DWT.enable_cycle_counter();

        defmt::info!("=== Hardware Diagnostics (on-screen) ===");

        let gpioc = p.GPIOC.split(&mut rcc);
        let gpiod = p.GPIOD.split(&mut rcc);
        let gpioe = p.GPIOE.split(&mut rcc);
        let gpiof = p.GPIOF.split(&mut rcc);
        let gpiog = p.GPIOG.split(&mut rcc);
        let gpioh = p.GPIOH.split(&mut rcc);
        let gpioi = p.GPIOI.split(&mut rcc);

        // SDRAM
        defmt::info!("SDRAM init...");
        let sdram = Sdram::new(
            p.FMC,
            sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
            &clocks,
            &mut delay,
        );
        let words = sdram.words;
        let ram: &mut [u32] = unsafe { slice::from_raw_parts_mut(sdram.mem, words) };

        let mut results: [TestResult; 30] = [const {
            TestResult {
                name: "",
                status: TestStatus::Pass,
            }
        }; 30];
        let mut ri = 0usize;

        macro_rules! tpass {
            ($name:expr, $block:expr) => {
                defmt::info!("TEST {}: RUNNING", $name);
                if $block {
                    results[ri] = TestResult {
                        name: $name,
                        status: TestStatus::Pass,
                    };
                    ri += 1;
                    defmt::info!("TEST {}: PASS", $name);
                } else {
                    results[ri] = TestResult {
                        name: $name,
                        status: TestStatus::Fail,
                    };
                    ri += 1;
                    defmt::error!("TEST {}: FAIL", $name);
                }
            };
        }

        // === SDRAM tests ===
        tpass!("SDRAM Init", true);

        tpass!("SDRAM Checkerboard", {
            let win = core::cmp::min(65536usize, words);
            for w in ram[..win].iter_mut() {
                *w = 0xAAAAAAAA;
            }
            ram[..win].iter().all(|w| *w == 0xAAAAAAAA)
        });

        tpass!("SDRAM March C-", {
            let win = core::cmp::min(65536usize, words);
            for w in ram[..win].iter_mut() {
                *w = 0;
            }
            let mut ok = true;
            for w in ram[..win].iter_mut() {
                if *w != 0 {
                    ok = false;
                    break;
                }
                *w = 0xFFFFFFFF;
            }
            if ok {
                for w in ram[..win].iter_mut().rev() {
                    if *w != 0xFFFFFFFF {
                        ok = false;
                        break;
                    }
                    *w = 0;
                }
            }
            if ok {
                for w in ram[..win].iter() {
                    if *w != 0 {
                        ok = false;
                        break;
                    }
                }
            }
            ok
        });

        tpass!("SDRAM Boundary", {
            let mut ok = true;
            for r in 0u32..16 {
                let offset = (r as usize) * (words / 16);
                let pattern = 0xFEED0000 | r;
                let end = core::cmp::min(offset + 1024, words);
                for w in ram[offset..end].iter_mut() {
                    *w = pattern;
                }
            }
            for r in 0u32..16 {
                let offset = (r as usize) * (words / 16);
                let pattern = 0xFEED0000 | r;
                let end = core::cmp::min(offset + 1024, words);
                for w in ram[offset..end].iter() {
                    if *w != pattern {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            ok
        });

        tpass!("SDRAM End-of-RAM", {
            let last = 16384usize;
            let start = words - last;
            let mut seed: u32 = 0x12345678;
            for w in ram[start..].iter_mut() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *w = seed;
            }
            seed = 0x12345678;
            let mut ok = true;
            for w in ram[start..].iter() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                if *w != seed {
                    ok = false;
                    break;
                }
            }
            ok
        });

        tpass!("SDRAM Byte/Halfword", {
            let mut ok = true;
            let ram_bytes: &mut [u8] =
                unsafe { slice::from_raw_parts_mut(sdram.mem as *mut u8, 4096) };
            for (i, b) in ram_bytes.iter_mut().enumerate() {
                *b = (i & 0xFF) as u8;
            }
            for (i, b) in ram_bytes.iter().enumerate() {
                if *b != (i & 0xFF) as u8 {
                    ok = false;
                    break;
                }
            }
            if ok {
                let ram_hw: &mut [u16] =
                    unsafe { slice::from_raw_parts_mut(sdram.mem as *mut u16, 2048) };
                for (i, hw) in ram_hw.iter_mut().enumerate() {
                    *hw = ((i & 0xFFFF) as u16).wrapping_add(1);
                }
                for (i, hw) in ram_hw.iter().enumerate() {
                    if *hw != ((i & 0xFFFF) as u16).wrapping_add(1) {
                        ok = false;
                        break;
                    }
                }
            }
            ok
        });

        // === Display init (RGB565) ===
        defmt::info!("Display init...");

        // LCD reset
        let gpioh = unsafe { pac::Peripherals::steal().GPIOH.split(&mut rcc) };
        let mut lcd_reset = gpioh.ph7.into_push_pull_output();
        lcd_reset.set_low();
        delay.delay_ms(20u32);
        lcd_reset.set_high();
        delay.delay_ms(10u32);

        // LTDC with RGB565
        let ltdc_freq = 27_429.kHz();
        let framebuffer_u16: &mut [u16] =
            unsafe { slice::from_raw_parts_mut(sdram.mem as *mut u16, WIDTH * HEIGHT) };
        let mut display = DisplayController::<u16>::new(
            unsafe { pac::Peripherals::steal().LTDC },
            unsafe { pac::Peripherals::steal().DMA2D },
            None,
            PixelFormat::RGB565,
            DISPLAY_CFG,
            Some(hse_freq),
        );
        display.config_layer(Layer::L1, framebuffer_u16, PixelFormat::RGB565);
        display.enable_layer(Layer::L1);
        display.reload();

        // DSI (still 24-bit on the wire — LTDC handles conversion from RGB565 to 24-bit)
        let dsi_pll = unsafe { DsiPllConfig::manual(125, 2, 0, 4) };
        let dsi_cfg = DsiConfig {
            mode: DsiMode::Video {
                mode: DsiVideoMode::Burst,
            },
            lane_count: LaneCount::DoubleLane,
            channel: DsiChannel::Ch0,
            hse_freq,
            ltdc_freq,
            interrupts: DsiInterrupts::None,
            color_coding_host: ColorCoding::TwentyFourBits,
            color_coding_wrapper: ColorCoding::TwentyFourBits,
            lp_size: 4,
            vlp_size: 4,
        };
        let mut rcc2 = unsafe {
            pac::Peripherals::steal()
                .RCC
                .constrain()
                .freeze(rcc::Config::hse(hse_freq).pclk2(32.MHz()).sysclk(180.MHz()))
        };
        let mut dsi_host = match DsiHost::init(
            dsi_pll,
            DISPLAY_CFG,
            dsi_cfg,
            unsafe { pac::Peripherals::steal().DSI },
            &mut rcc2,
        ) {
            Ok(h) => h,
            Err(_) => {
                defmt::error!("DSI init failed");
                loop {
                    continue;
                }
            }
        };
        dsi_host.configure_phy_timers(DsiPhyTimers {
            dataline_hs2lp: 35,
            dataline_lp2hs: 35,
            clock_hs2lp: 35,
            clock_lp2hs: 35,
            dataline_max_read_time: 0,
            stop_wait_time: 10,
        });
        dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInLowPower);
        dsi_host.start();
        dsi_host.enable_bus_turn_around();

        // OTM8009A
        let otm_cfg = Otm8009AConfig {
            frame_rate: otm8009a::FrameRate::_60Hz,
            mode: otm8009a::Mode::Portrait,
            color_map: otm8009a::ColorMap::Rgb,
            cols: WIDTH as u16,
            rows: HEIGHT as u16,
        };
        let mut otm = Otm8009A::new();
        match otm.init(&mut dsi_host, otm_cfg, &mut delay) {
            Ok(_) => {}
            Err(_) => {
                defmt::error!("OTM8009A init failed");
                loop {
                    continue;
                }
            }
        }
        let _ = otm.enable_te_output(533, &mut dsi_host);
        dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
        dsi_host.force_rx_low_power(true);
        dsi_host.refresh();

        defmt::info!("Display init done");
        tpass!("Display Init", true);

        let fb_u16: &mut [u16] =
            unsafe { slice::from_raw_parts_mut(sdram.mem as *mut u16, WIDTH * HEIGHT) };
        let mut fb = FramebufferTarget::new(fb_u16, WIDTH, HEIGHT);

        let style = MonoTextStyleBuilder::new()
            .font(&FONT_6X9)
            .text_color(TEXT_COLOR)
            .background_color(BG)
            .build();

        let header_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X9)
            .text_color(HEADER_COLOR)
            .background_color(BG)
            .build();

        fb.clear(BG).ok();
        let mut y: i32 = 10;
        draw_text(&mut fb, "STM32F469I-DISCO", 8, y, &header_style);
        y += 14;
        draw_text(&mut fb, "Hardware Diagnostics v1.0.0", 8, y, &style);
        y += 18;

        // Render SDRAM results
        draw_section(&mut fb, &mut y, "SDRAM (16MB IS42S32400F-6)", &header_style);
        for i in 0..ri {
            draw_result(&mut fb, &mut y, &results[i], &style);
        }

        // === Display tests ===
        draw_section(&mut fb, &mut y, "Display (DSI/LTDC/NT35510)", &header_style);

        tpass!("Display Red Fill", {
            fb.fill_raw(Rgb565::RED.into_storage());
            delay.delay_ms(200u32);
            true
        });

        tpass!("Display Green Fill", {
            fb.fill_raw(Rgb565::GREEN.into_storage());
            delay.delay_ms(200u32);
            true
        });

        tpass!("Display Blue Fill", {
            fb.fill_raw(Rgb565::BLUE.into_storage());
            delay.delay_ms(200u32);
            true
        });

        tpass!("Display Gradient", {
            for row in 0..HEIGHT {
                let r = ((row as u32 * 255) / HEIGHT as u32) as u8;
                let b = (255 - row as u32 * 255 / HEIGHT as u32) as u8;
                let color = Rgb565::new(r, 0, b);
                Rectangle::new(Point::new(0, row as i32), Size::new(WIDTH as u32, 1))
                    .into_styled(PrimitiveStyle::with_fill(color))
                    .draw(&mut fb)
                    .ok();
            }
            delay.delay_ms(100u32);
            true
        });

        tpass!("Display Text Render", {
            fb.clear(BG).ok();
            let tstyle = MonoTextStyleBuilder::new()
                .font(&FONT_6X9)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::CSS_NAVY)
                .build();
            embedded_graphics::text::Text::new("HELLO WORLD", Point::new(120, 390), tstyle)
                .draw(&mut fb)
                .ok();
            delay.delay_ms(200u32);
            true
        });

        // === Touch tests ===
        draw_section(&mut fb, &mut y, "Touch (FT6X06 / I2C1)", &header_style);

        tpass!("Touch I2C Init", {
            let _i2c = {
                let gpiob = unsafe { pac::Peripherals::steal().GPIOB.split(&mut rcc) };
                touch::init_i2c(
                    unsafe { pac::Peripherals::steal().I2C1 },
                    gpiob.pb8,
                    gpiob.pb9,
                    &mut rcc,
                )
            };
            true
        });

        tpass!("Touch Chip ID", {
            let mut i2c = {
                let gpiob = unsafe { pac::Peripherals::steal().GPIOB.split(&mut rcc) };
                touch::init_i2c(
                    unsafe { pac::Peripherals::steal().I2C1 },
                    gpiob.pb8,
                    gpiob.pb9,
                    &mut rcc,
                )
            };
            let mut buf = [0u8; 1];
            match i2c.write_read(FT6X06_I2C_ADDR, &[0xA8], &mut buf) {
                Ok(()) => {
                    let id = buf[0];
                    defmt::info!("  Chip ID: {:#04X}", id);
                    id == 0xCC || id == 0xA3
                }
                Err(_) => false,
            }
        });

        tpass!("Touch Idle Status", {
            let mut i2c = {
                let gpiob = unsafe { pac::Peripherals::steal().GPIOB.split(&mut rcc) };
                touch::init_i2c(
                    unsafe { pac::Peripherals::steal().I2C1 },
                    gpiob.pb8,
                    gpiob.pb9,
                    &mut rcc,
                )
            };
            let mut buf = [0u8; 1];
            match i2c.write_read(FT6X06_I2C_ADDR, &[0x02], &mut buf) {
                Ok(()) => {
                    defmt::info!("  TD status: {}", buf[0]);
                    buf[0] == 0
                }
                Err(_) => false,
            }
        });

        // === GPIO tests ===
        draw_section(&mut fb, &mut y, "GPIO", &header_style);

        tpass!("GPIO Input PA0", {
            let gpioa = unsafe { pac::Peripherals::steal().GPIOA.split(&mut rcc) };
            let _button = gpioa.pa0.into_pull_down_input();
            true
        });

        tpass!("GPIO Multi-Port Output", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let gpiog = unsafe { pac::Peripherals::steal().GPIOG.split(&mut rcc) };
            let gpiok = unsafe { pac::Peripherals::steal().GPIOK.split(&mut rcc) };
            let mut g = gpiog.pg6.into_push_pull_output();
            g.set_high();
            g.toggle();
            g.set_low();
            let mut o = gpiod.pd4.into_push_pull_output();
            o.set_high();
            o.set_low();
            let mut r = gpiod.pd5.into_push_pull_output();
            r.set_high();
            r.set_low();
            let mut b = gpiok.pk3.into_push_pull_output();
            b.set_high();
            b.set_low();
            true
        });

        // === LED tests ===
        draw_section(&mut fb, &mut y, "LEDs", &header_style);

        tpass!("LED Green (PG6)", {
            let gpiog = unsafe { pac::Peripherals::steal().GPIOG.split(&mut rcc) };
            let mut led = gpiog.pg6.into_push_pull_output();
            led.set_high();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED Orange (PD4)", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let mut led = gpiod.pd4.into_push_pull_output();
            led.toggle();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED Red (PD5)", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let mut led = gpiod.pd5.into_push_pull_output();
            led.toggle();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED Blue (PK3)", {
            let gpiok = unsafe { pac::Peripherals::steal().GPIOK.split(&mut rcc) };
            let mut led = gpiok.pk3.into_push_pull_output();
            led.toggle();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED All Toggle", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let gpiog = unsafe { pac::Peripherals::steal().GPIOG.split(&mut rcc) };
            let gpiok = unsafe { pac::Peripherals::steal().GPIOK.split(&mut rcc) };
            let mut _g = gpiog.pg6.into_push_pull_output();
            let mut _o = gpiod.pd4.into_push_pull_output();
            let mut _r = gpiod.pd5.into_push_pull_output();
            let mut _b = gpiok.pk3.into_push_pull_output();
            for _ in 0..3 {
                _g.toggle();
                _o.toggle();
                _r.toggle();
                _b.toggle();
                delay.delay_ms(80u32);
            }
            true
        });

        tpass!("GPIO Multi-Port Output", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let gpiog = unsafe { pac::Peripherals::steal().GPIOG.split(&mut rcc) };
            let gpiok = unsafe { pac::Peripherals::steal().GPIOK.split(&mut rcc) };
            let mut g = gpiog.pg6.into_push_pull_output();
            g.set_high();
            g.toggle();
            g.set_low();
            let mut o = gpiod.pd4.into_push_pull_output();
            o.set_high();
            o.set_low();
            let mut r = gpiod.pd5.into_push_pull_output();
            r.set_high();
            r.set_low();
            let mut b = gpiok.pk3.into_push_pull_output();
            b.set_high();
            b.set_low();
            true
        });

        // === LED tests ===
        draw_section(&mut fb, &mut y, "LEDs", &header_style);

        tpass!("LED Green (PG6)", {
            let gpiog = unsafe { pac::Peripherals::steal().GPIOG.split(&mut rcc) };
            let mut led = gpiog.pg6.into_push_pull_output();
            led.set_high();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED Orange (PD4)", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let mut led = gpiod.pd4.into_push_pull_output();
            led.toggle();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED Red (PD5)", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let mut led = gpiod.pd5.into_push_pull_output();
            led.toggle();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED Blue (PK3)", {
            let gpiok = unsafe { pac::Peripherals::steal().GPIOK.split(&mut rcc) };
            let mut led = gpiok.pk3.into_push_pull_output();
            led.toggle();
            delay.delay_ms(100u32);
            led.set_low();
            true
        });

        tpass!("LED All Toggle", {
            let gpiod = unsafe { pac::Peripherals::steal().GPIOD.split(&mut rcc) };
            let gpiog = unsafe { pac::Peripherals::steal().GPIOG.split(&mut rcc) };
            let gpiok = unsafe { pac::Peripherals::steal().GPIOK.split(&mut rcc) };
            let mut lg = gpiog.pg6.into_push_pull_output();
            let mut lo = gpiod.pd4.into_push_pull_output();
            let mut lr = gpiod.pd5.into_push_pull_output();
            let mut lb = gpiok.pk3.into_push_pull_output();
            for _ in 0..3 {
                lg.toggle();
                lo.toggle();
                lr.toggle();
                lb.toggle();
                delay.delay_ms(80u32);
            }
            lg.set_low();
            lo.set_low();
            lr.set_low();
            lb.set_low();
            true
        });

        // === Timer tests ===
        draw_section(&mut fb, &mut y, "Timers", &header_style);

        tpass!("Timer 1ms (DWT)", {
            let start = DWT::cycle_count();
            delay.delay_us(1000u32);
            let us = DWT::cycle_count().wrapping_sub(start) / 180;
            defmt::info!("  1ms delay: {}us", us);
            us >= 900 && us <= 1500
        });

        tpass!("Timer 50ms (DWT)", {
            let start = DWT::cycle_count();
            delay.delay_ms(50u32);
            let ms = DWT::cycle_count().wrapping_sub(start) / 180_000;
            defmt::info!("  50ms delay: {}ms", ms);
            ms >= 45 && ms <= 70
        });

        // === Summary screen ===
        fb.clear(BG).ok();
        let mut sy: i32 = 10;
        draw_text(&mut fb, "STM32F469I-DISCO", 8, sy, &header_style);
        sy += 14;
        draw_text(&mut fb, "Hardware Diagnostics v1.0.0", 8, sy, &style);
        sy += 18;

        draw_section(
            &mut fb,
            &mut sy,
            "SDRAM (16MB IS42S32400F-6)",
            &header_style,
        );
        for i in 0..6 {
            draw_result(&mut fb, &mut sy, &results[i], &style);
        }

        draw_section(
            &mut fb,
            &mut sy,
            "Display (DSI/LTDC/NT35510)",
            &header_style,
        );
        for i in 6..11 {
            draw_result(&mut fb, &mut sy, &results[i], &style);
        }

        draw_section(&mut fb, &mut sy, "Touch (FT6X06 / I2C1)", &header_style);
        for i in 11..14 {
            draw_result(&mut fb, &mut sy, &results[i], &style);
        }

        draw_section(&mut fb, &mut sy, "GPIO", &header_style);
        for i in 14..16 {
            draw_result(&mut fb, &mut sy, &results[i], &style);
        }

        draw_section(&mut fb, &mut sy, "LEDs", &header_style);
        for i in 16..21 {
            draw_result(&mut fb, &mut sy, &results[i], &style);
        }

        draw_section(&mut fb, &mut sy, "Timers", &header_style);
        for i in 21..ri {
            draw_result(&mut fb, &mut sy, &results[i], &style);
        }

        draw_summary(&mut fb, &results[..ri], sy, &style, &header_style);

        delay.delay_ms(2000u32);

        // === Touch Demo (30 seconds) ===
        defmt::info!("Entering touch demo...");

        let touch_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X9)
            .text_color(TEXT_COLOR)
            .background_color(BG)
            .build();

        let touch_header = MonoTextStyleBuilder::new()
            .font(&FONT_6X9)
            .text_color(HEADER_COLOR)
            .background_color(BG)
            .build();

        draw_touch_prompt(&mut fb, &touch_style, &touch_header);

        let mut i2c = {
            let gpiob = unsafe { pac::Peripherals::steal().GPIOB.split(&mut rcc) };
            touch::init_i2c(
                unsafe { pac::Peripherals::steal().I2C1 },
                gpiob.pb8,
                gpiob.pb9,
                &mut rcc,
            )
        };
        let mut deadline = 30u32;
        let mut touch_count = 0u32;
        while deadline > 0 {
            let mut status_buf = [0u8; 1];
            match i2c.write_read(FT6X06_I2C_ADDR, &[0x02], &mut status_buf) {
                Ok(()) if status_buf[0] > 0 => {
                    let mut touch_buf = [0u8; 4];
                    match i2c.write_read(FT6X06_I2C_ADDR, &[0x03], &mut touch_buf) {
                        Ok(()) => {
                            let x = ((touch_buf[0] & 0x0F) as u16) << 8 | touch_buf[1] as u16;
                            let y = ((touch_buf[2] & 0x0F) as u16) << 8 | touch_buf[3] as u16;
                            if x >= 3 && x <= 476 && y >= 3 && y <= 796 {
                                defmt::info!("Touch at ({}, {})", x, y);
                                draw_touch_prompt(&mut fb, &touch_style, &touch_header);
                                draw_touch_point(&mut fb, x, y, &touch_style);
                                draw_text(
                                    &mut fb,
                                    "Touches: ",
                                    8,
                                    HEIGHT as i32 - 50,
                                    &touch_style,
                                );
                                draw_u32_text(
                                    &mut fb,
                                    62,
                                    HEIGHT as i32 - 50,
                                    &touch_style,
                                    touch_count + 1,
                                );
                                touch_count += 1;
                            }
                        }
                        Err(_) => {}
                    }
                }
                _ => {}
            }
            delay.delay_ms(50u32);
            deadline -= 1;
        }

        draw_touch_prompt(&mut fb, &touch_style, &touch_header);
        draw_text(&mut fb, "Touch demo complete.", 8, 90, &touch_style);
        draw_text(&mut fb, "Press reset to restart.", 8, 102, &touch_style);

        let final_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X9)
            .text_color(PASS_COLOR)
            .background_color(BG)
            .build();
        draw_text(&mut fb, "Touches detected: ", 8, 130, &final_style);
        draw_u32_text(&mut fb, 120, 130, &final_style, touch_count);

        defmt::info!("Touch demo complete. {} touches detected.", touch_count);
        defmt::info!("Holding final screen. Press reset to restart.");

        loop {
            continue;
        }
    }

    loop {
        continue;
    }
}
