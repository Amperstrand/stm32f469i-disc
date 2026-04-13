//! ARGB8888 register dump diagnostic firmware for STM32F469I-DISCO.
//!
//! Runs `init_display_full_argb8888()` then reads and displays key hardware
//! register values on-screen. Also draws the pixel ruler below the register
//! dump so the shift can be measured simultaneously.
//!
//! NO RTT, NO defmt, NO panic_probe — output is on the display only.
//!
//! Flash with:
//!   arm-none-eabi-objcopy -O binary \
//!     target/thumbv7em-none-eabihf/release/examples/display_regdump \
//!     /tmp/display_regdump.bin
//!   pkill -9 probe-rs; sleep 3
//!   sudo st-flash --connect-under-reset write /tmp/display_regdump.bin 0x08000000
//!   sudo st-flash --connect-under-reset reset

#![no_main]
#![no_std]

use cortex_m_rt::entry;
use panic_halt as _;

use stm32f469i_disc as board;

use board::hal::gpio::alt::fmc as alt;
use board::hal::ltdc::{Layer, PixelFormat};
use board::hal::pac::{CorePeripherals, Peripherals};
use board::hal::{prelude::*, rcc};
use board::lcd::{self, DisplayOrientation, FramebufferView};
use board::sdram::{sdram_pins, Sdram};

use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::Text,
};

const W: u32 = 480;
const H: u32 = 800;

// Simple u32 → decimal ASCII (no std/alloc)
fn fmt_u32(n: u32, buf: &mut [u8; 12]) -> &str {
    if n == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap();
    }
    let mut pos = 12usize;
    let mut v = n;
    while v > 0 {
        pos -= 1;
        buf[pos] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    core::str::from_utf8(&buf[pos..]).unwrap()
}

// Simple u32 → "0xHHHHHHHH" hex ASCII
fn fmt_hex(n: u32, buf: &mut [u8; 10]) -> &str {
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..8 {
        let nibble = ((n >> (28 - i * 4)) & 0xF) as u8;
        buf[2 + i] = if nibble < 10 {
            b'0' + nibble
        } else {
            b'a' + nibble - 10
        };
    }
    core::str::from_utf8(buf).unwrap()
}

// Concatenate two string slices into a fixed buffer
fn concat<'a>(a: &str, b: &str, buf: &'a mut [u8; 64]) -> &'a str {
    let ab = a.len();
    let bb = b.len();
    let total = (ab + bb).min(63);
    buf[..ab.min(63)].copy_from_slice(&a.as_bytes()[..ab.min(63)]);
    if ab < 63 {
        let take = bb.min(63 - ab);
        buf[ab..ab + take].copy_from_slice(&b.as_bytes()[..take]);
    }
    core::str::from_utf8(&buf[..total]).unwrap()
}

#[entry]
fn main() -> ! {
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

    // Run the full ARGB8888 init path — this is what we're diagnosing
    let (mut display_ctrl, _controller, _orientation) = lcd::init_display_full_argb8888(
        dp.DSI,
        dp.LTDC,
        dp.DMA2D,
        &mut rcc,
        &mut delay,
        lcd::BoardHint::ForceNt35510,
        orientation,
    );

    // ── Read hardware registers AFTER init ───────────────────────────────────
    // SAFETY: We are single-threaded, and we only read (no writes) from these
    // peripherals after init. The init functions are done before this block.
    let (pllsaicfgr_val, dckcfgr_val, hline, hsa, hbp, wcfgr_val, lcolcr_val, vpsize) = unsafe {
        let rcc_p = &*board::hal::pac::RCC::ptr();
        let dsi_p = &*board::hal::pac::DSI::ptr();

        let pllsaicfgr = rcc_p.pllsaicfgr().read().bits();
        let dckcfgr = rcc_p.dckcfgr().read().bits();
        let hline_v = dsi_p.vlcr().read().bits() & 0x7FFF;
        let hsa_v = dsi_p.vhsacr().read().bits() & 0xFFF;
        let hbp_v = dsi_p.vhbpcr().read().bits() & 0xFFF;
        let wcfgr = dsi_p.wcfgr().read().bits();
        let lcolcr = dsi_p.lcolcr().read().bits();
        let vp = dsi_p.vpcr().read().bits() & 0x3FFF;

        (
            pllsaicfgr, dckcfgr, hline_v, hsa_v, hbp_v, wcfgr, lcolcr, vp,
        )
    };

    // Extract fields
    let plln = (pllsaicfgr_val >> 6) & 0x1FF;
    let pllr = (pllsaicfgr_val >> 28) & 0x7;
    let pllsaidivr_raw = (dckcfgr_val >> 16) & 0x3;
    // PLLSAIDIVR encoding: 0=/2, 1=/4, 2=/8, 3=/16
    let divr_actual: u32 = 1u32 << (pllsaidivr_raw + 1);
    let colmux = (wcfgr_val >> 1) & 0x7;
    let colc = lcolcr_val & 0xF;

    // Computed values
    // pixel_clock_khz = (HSE_kHz * PLLN) / (PLLM * PLLR * DIVR)
    // HSE=8MHz, PLLM=8 (from rcc::Config::hse(8.MHz()))
    // => (8000 * plln) / (8 * pllr * divr_actual)
    let pixel_clock_khz = if pllr > 0 && divr_actual > 0 {
        (8_000u32 * plln) / (8 * pllr * divr_actual)
    } else {
        0
    };
    // HFP = HLINE - HSA - HBP - HACT_bytes  (ARGB8888: 480px * 4 bytes/pix / 3 lanes ≈ 720 byte-clocks? No: HACT_bytes = pixels * bytes_per_pixel. For 24bpp COLMUX=5, HACT_bytes=720)
    let hact_24bpp: u32 = 480 * 3; // 24bpp DSI: HACT = PANEL_WIDTH * 3 byte-clocks = 1440
    let der_hfp = if hline > hsa + hbp + hact_24bpp {
        hline - hsa - hbp - hact_24bpp
    } else {
        0 // would be negative — indicates problem
    };
    // Also compute with the common 720 assumption (480*4/2 = wrong; just show both)
    let der_hfp_720 = if hline > hsa + hbp + 720 {
        hline - hsa - hbp - 720
    } else {
        0
    };

    // ── Render to framebuffer ────────────────────────────────────────────────
    let buffer: &'static mut [u32] = sdram.subslice_mut(0, orientation.fb_size());
    let buffer_addr = buffer.as_mut_ptr() as usize;

    {
        let mut fb = FramebufferView::new(buffer, W, H);
        fb.clear(Rgb888::BLACK);

        let white_sm = MonoTextStyle::new(&FONT_6X10, Rgb888::WHITE);
        let white_lg = MonoTextStyle::new(&FONT_10X20, Rgb888::WHITE);
        let yellow = MonoTextStyle::new(&FONT_6X10, Rgb888::YELLOW);
        let cyan = MonoTextStyle::new(&FONT_6X10, Rgb888::CYAN);
        let green = MonoTextStyle::new(&FONT_6X10, Rgb888::GREEN);

        // Title
        Text::new("== ARGB8888 REGISTER DUMP ==", Point::new(2, 12), white_lg)
            .draw(&mut fb)
            .ok();

        let mut y = 35i32;
        let dy = 14i32;

        // Helper to draw a label+value line
        macro_rules! line {
            ($style:expr, $label:expr, $val:expr) => {{
                let mut buf64 = [0u8; 64];
                let s = concat($label, $val, &mut buf64);
                Text::new(s, Point::new(2, y), $style).draw(&mut fb).ok();
                y += dy;
            }};
        }

        // PLLSAICFGR
        let mut hx = [0u8; 10];
        line!(yellow, "PLLSAICFGR: ", fmt_hex(pllsaicfgr_val, &mut hx));
        let mut nb = [0u8; 12];
        let mut nb2 = [0u8; 12];
        {
            let mut buf = [0u8; 64];
            let plln_s = fmt_u32(plln, &mut nb);
            let pllr_s = fmt_u32(pllr, &mut nb2);
            let a = concat("  PLLN=", plln_s, &mut buf);
            let mut buf2 = [0u8; 64];
            let s = concat(a, "  PLLR=", &mut buf2);
            let mut buf3 = [0u8; 64];
            let s2 = concat(s, pllr_s, &mut buf3);
            Text::new(s2, Point::new(2, y), cyan).draw(&mut fb).ok();
            y += dy;
        }

        // DCKCFGR
        let mut hx = [0u8; 10];
        line!(yellow, "DCKCFGR:    ", fmt_hex(dckcfgr_val, &mut hx));
        {
            let mut nb = [0u8; 12];
            let mut nb2 = [0u8; 12];
            let divr_s = fmt_u32(divr_actual, &mut nb);
            let raw_s = fmt_u32(pllsaidivr_raw, &mut nb2);
            let mut buf = [0u8; 64];
            let a = concat("  PLLSAIDIVR_raw=", raw_s, &mut buf);
            let mut buf2 = [0u8; 64];
            let s = concat(a, "  /", &mut buf2);
            let mut buf3 = [0u8; 64];
            let s2 = concat(s, divr_s, &mut buf3);
            Text::new(s2, Point::new(2, y), cyan).draw(&mut fb).ok();
            y += dy;
        }

        // Pixel clock
        {
            let mut nb = [0u8; 12];
            let clk_s = fmt_u32(pixel_clock_khz, &mut nb);
            let mut buf = [0u8; 64];
            let s = concat("pix_clk: ", clk_s, &mut buf);
            let mut buf2 = [0u8; 64];
            let s2 = concat(s, " kHz  (embassy=27429)", &mut buf2);
            let style = if pixel_clock_khz >= 27_000 && pixel_clock_khz <= 27_900 {
                green
            } else {
                MonoTextStyle::new(&FONT_6X10, Rgb888::RED)
            };
            Text::new(s2, Point::new(2, y), style).draw(&mut fb).ok();
            y += dy;
        }

        y += 4;

        // DSI timing
        {
            let mut nb1 = [0u8; 12];
            let mut nb2 = [0u8; 12];
            let mut nb3 = [0u8; 12];
            let hline_s = fmt_u32(hline, &mut nb1);
            let hsa_s = fmt_u32(hsa, &mut nb2);
            let hbp_s = fmt_u32(hbp, &mut nb3);
            let mut buf = [0u8; 64];
            let a = concat("HLINE: ", hline_s, &mut buf);
            let mut buf2 = [0u8; 64];
            let b = concat(a, "  HSA: ", &mut buf2);
            let mut buf3 = [0u8; 64];
            let c = concat(b, hsa_s, &mut buf3);
            let mut buf4 = [0u8; 64];
            let d = concat(c, "  HBP: ", &mut buf4);
            let mut buf5 = [0u8; 64];
            let e = concat(d, hbp_s, &mut buf5);
            Text::new(e, Point::new(2, y), white_sm).draw(&mut fb).ok();
            y += dy;
        }

        // Derived HFP (24bpp = 1440 HACT_bytes)
        {
            let mut nb = [0u8; 12];
            let hfp_s = fmt_u32(der_hfp, &mut nb);
            let mut buf = [0u8; 64];
            let s = concat("HFP(24bpp,1440): ", hfp_s, &mut buf);
            let style = if der_hfp > 0 {
                green
            } else {
                MonoTextStyle::new(&FONT_6X10, Rgb888::RED)
            };
            Text::new(s, Point::new(2, y), style).draw(&mut fb).ok();
            y += dy;
        }
        {
            let mut nb = [0u8; 12];
            let hfp_s = fmt_u32(der_hfp_720, &mut nb);
            let mut buf = [0u8; 64];
            let s = concat("HFP(720): ", hfp_s, &mut buf);
            let style = if der_hfp_720 > 0 {
                green
            } else {
                MonoTextStyle::new(&FONT_6X10, Rgb888::RED)
            };
            Text::new(s, Point::new(2, y), style).draw(&mut fb).ok();
            y += dy;
        }

        y += 4;

        // COLMUX / COLC
        {
            let mut nb1 = [0u8; 12];
            let mut nb2 = [0u8; 12];
            let mut hx1 = [0u8; 10];
            let mut hx2 = [0u8; 10];
            let colmux_s = fmt_u32(colmux, &mut nb1);
            let colc_s = fmt_u32(colc, &mut nb2);
            let wcfgr_hex = fmt_hex(wcfgr_val, &mut hx1);
            let lcolcr_hex = fmt_hex(lcolcr_val, &mut hx2);
            let mut buf = [0u8; 64];
            let a = concat("WCFGR: ", wcfgr_hex, &mut buf);
            let mut buf2 = [0u8; 64];
            let b = concat(a, "  COLMUX=", &mut buf2);
            let mut buf3 = [0u8; 64];
            let c = concat(b, colmux_s, &mut buf3);
            // COLMUX=5 means 24-bit (expected for TwentyFourBits)
            let style = if colmux == 5 {
                green
            } else {
                MonoTextStyle::new(&FONT_6X10, Rgb888::RED)
            };
            Text::new(c, Point::new(2, y), style).draw(&mut fb).ok();
            y += dy;

            let mut buf4 = [0u8; 64];
            let d = concat("LCOLCR: ", lcolcr_hex, &mut buf4);
            let mut buf5 = [0u8; 64];
            let e = concat(d, "  COLC=", &mut buf5);
            let mut buf6 = [0u8; 64];
            let f = concat(e, colc_s, &mut buf6);
            let style2 = if colc == 5 {
                green
            } else {
                MonoTextStyle::new(&FONT_6X10, Rgb888::RED)
            };
            Text::new(f, Point::new(2, y), style2).draw(&mut fb).ok();
            y += dy;
        }

        // VPSIZE
        {
            let mut nb = [0u8; 12];
            let vp_s = fmt_u32(vpsize, &mut nb);
            let mut buf = [0u8; 64];
            let s = concat("VPSIZE: ", vp_s, &mut buf);
            let mut buf2 = [0u8; 64];
            let s2 = concat(s, "  (expected: 480)", &mut buf2);
            let style = if vpsize == 480 {
                green
            } else {
                MonoTextStyle::new(&FONT_6X10, Rgb888::RED)
            };
            Text::new(s2, Point::new(2, y), style).draw(&mut fb).ok();
            y += dy;
        }

        y += 8;

        // Separator line
        Line::new(Point::new(0, y), Point::new(W as i32 - 1, y))
            .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(80, 80, 80), 1))
            .draw(&mut fb)
            .ok();
        y += 4;

        // ── Pixel ruler (copy from display_cross_test_t5.rs, offset y) ──────
        let ruler_top = y;

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
            Rectangle::new(
                Point::new(x, ruler_top + 20),
                Size::new(w, H - ruler_top as u32 - 20),
            )
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(&mut fb)
            .ok();
            if !label.is_empty() {
                let t = if color == Rgb888::BLACK {
                    white_lg
                } else {
                    MonoTextStyle::new(&FONT_10X20, Rgb888::BLACK)
                };
                Text::new(label, Point::new(x + w as i32 / 2 - 10, ruler_top + 32), t)
                    .draw(&mut fb)
                    .ok();
            }
        }

        // Horizontal ruler line
        Line::new(
            Point::new(0, ruler_top + 19),
            Point::new(W as i32 - 1, ruler_top + 19),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::WHITE, 2))
        .draw(&mut fb)
        .ok();

        Text::new(
            "ARGB8888 SHIFT RULER",
            Point::new(120, ruler_top + 10),
            white_sm,
        )
        .draw(&mut fb)
        .ok();

        // Horizontal grid lines every 80px within ruler area
        let ruler_bottom = H as i32;
        let mut gy = ruler_top;
        while gy < ruler_bottom {
            Line::new(Point::new(0, gy), Point::new(W as i32 - 1, gy))
                .into_styled(PrimitiveStyle::with_stroke(Rgb888::new(64, 64, 64), 1))
                .draw(&mut fb)
                .ok();
            gy += 80;
        }
    }

    // Configure LTDC layer with framebuffer
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
