//! LCD/DSI display test for STM32F469I-DISCO
//!
//! Tests:
//!   1. SDRAM init (framebuffer backing)
//!   2. LCD reset sequence
//!   3. LTDC display controller init
//!   4. DSI host init and PLL
//!   5. OTM8009A init
//!   6. Solid color fill (red)
//!   7. Solid color fill (green)
//!   8. Solid color fill (blue)
//!   9. Gradient fill (no crash = pass)
//!   10. Continuous display loop (stability)
//!
//! Visual confirmation required for color tests.

#![no_main]
#![no_std]

extern crate cortex_m_rt as rt;

use stm32f469i_disc as board;

use core::slice;

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::{entry, exception};

use defmt_rtt as _;
use panic_probe as _;

use crate::board::hal::gpio::alt::fmc as alt;
use crate::board::hal::{pac, prelude::*, rcc};
use crate::board::sdram::{sdram_pins, Sdram};

use stm32f4xx_hal::ltdc::{DisplayConfig, DisplayController, Layer, PixelFormat};

use otm8009a::{Otm8009A, Otm8009AConfig};

use stm32f4xx_hal::dsi::{
    ColorCoding, DsiChannel, DsiCmdModeTransmissionKind, DsiConfig, DsiHost, DsiInterrupts,
    DsiMode, DsiPhyTimers, DsiPllConfig, DsiVideoMode, LaneCount,
};

use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

pub const DISPLAY_CONFIGURATION: DisplayConfig = DisplayConfig {
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

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

fn fail(name: &str, reason: &str) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!("TEST {}: FAIL {}", name, reason);
}

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut cp = Peripherals::take().unwrap();

    let rcc = dp.RCC.constrain();

    let hse_freq = 8.MHz();
    let mut rcc = rcc.freeze(rcc::Config::hse(hse_freq).pclk2(32.MHz()).sysclk(180.MHz()));
    let clocks = rcc.clocks;
    let mut delay = cp.SYST.delay(&clocks);

    cp.SCB.invalidate_icache();
    cp.SCB.enable_icache();

    let _gpioa = dp.GPIOA.split(&mut rcc);
    let gpioc = dp.GPIOC.split(&mut rcc);
    let gpiod = dp.GPIOD.split(&mut rcc);
    let gpioe = dp.GPIOE.split(&mut rcc);
    let gpiof = dp.GPIOF.split(&mut rcc);
    let gpiog = dp.GPIOG.split(&mut rcc);
    let gpioh = dp.GPIOH.split(&mut rcc);
    let gpioi = dp.GPIOI.split(&mut rcc);

    defmt::info!("=== LCD Test Suite ===");

    // Test 1: SDRAM init
    defmt::info!("TEST sdram_init: RUNNING");
    let sdram = Sdram::new(
        dp.FMC,
        sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
        &clocks,
        &mut delay,
    );
    pass("sdram_init");

    let framebuffer = unsafe { slice::from_raw_parts_mut(sdram.mem, WIDTH * HEIGHT) };

    // Test 2: LCD reset
    defmt::info!("TEST lcd_reset: RUNNING");
    let mut lcd_reset = gpioh.ph7.into_push_pull_output();
    lcd_reset.set_low();
    delay.delay_ms(20u32);
    lcd_reset.set_high();
    delay.delay_ms(10u32);
    pass("lcd_reset");

    // Test 3: LTDC init
    defmt::info!("TEST ltdc_init: RUNNING");
    let ltdc_freq = 27_429.kHz();
    let mut display = DisplayController::<u32>::new(
        dp.LTDC,
        dp.DMA2D,
        None,
        PixelFormat::ARGB8888,
        DISPLAY_CONFIGURATION,
        Some(hse_freq),
    );
    display.config_layer(Layer::L1, framebuffer, PixelFormat::ARGB8888);
    display.enable_layer(Layer::L1);
    display.reload();
    pass("ltdc_init");

    // Test 4: DSI host init
    defmt::info!("TEST dsi_init: RUNNING");
    let dsi_pll_config = unsafe { DsiPllConfig::manual(125, 2, 0, 4) };

    let dsi_config = DsiConfig {
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

    let mut dsi_host = match DsiHost::init(
        dsi_pll_config,
        DISPLAY_CONFIGURATION,
        dsi_config,
        dp.DSI,
        &mut rcc,
    ) {
        Ok(h) => {
            pass("dsi_init");
            h
        }
        Err(e) => {
            fail("dsi_init", "DSI host init returned error");
            defmt::error!("DSI error: {:?}", e);
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

    // Test 5: OTM8009A init
    defmt::info!("TEST otm8009a_init: RUNNING");
    let otm8009a_config = Otm8009AConfig {
        frame_rate: otm8009a::FrameRate::_60Hz,
        mode: otm8009a::Mode::Portrait,
        color_map: otm8009a::ColorMap::Rgb,
        cols: WIDTH as u16,
        rows: HEIGHT as u16,
    };
    let mut otm8009a = Otm8009A::new();
    match otm8009a.init(&mut dsi_host, otm8009a_config, &mut delay) {
        Ok(_) => {
            pass("otm8009a_init");
        }
        Err(e) => {
            fail("otm8009a_init", "OTM8009A init failed");
            defmt::error!("OTM8009A error: {:?}", e);
            loop {
                continue;
            }
        }
    }
    match otm8009a.enable_te_output(533, &mut dsi_host) {
        Ok(_) => {}
        Err(_) => {
            defmt::warn!("TE output enable failed (non-fatal)");
        }
    }

    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
    dsi_host.force_rx_low_power(true);
    dsi_host.refresh();

    let fb = unsafe { slice::from_raw_parts_mut(sdram.mem, sdram.words) };

    // Test 6: Solid red fill
    defmt::info!("TEST fill_red: RUNNING");
    for pixel in fb.iter_mut().take(WIDTH * HEIGHT) {
        *pixel = 0x00FF0000;
    }
    delay.delay_ms(500u32);
    pass("fill_red");

    // Test 7: Solid green fill
    defmt::info!("TEST fill_green: RUNNING");
    for pixel in fb.iter_mut().take(WIDTH * HEIGHT) {
        *pixel = 0x0000FF00;
    }
    delay.delay_ms(500u32);
    pass("fill_green");

    // Test 8: Solid blue fill
    defmt::info!("TEST fill_blue: RUNNING");
    for pixel in fb.iter_mut().take(WIDTH * HEIGHT) {
        *pixel = 0x000000FF;
    }
    delay.delay_ms(500u32);
    pass("fill_blue");

    // Test 9: Solid white fill
    defmt::info!("TEST fill_white: RUNNING");
    for pixel in fb.iter_mut().take(WIDTH * HEIGHT) {
        *pixel = 0x00FFFFFF;
    }
    delay.delay_ms(500u32);
    pass("fill_white");

    // Test 10: Solid black fill
    defmt::info!("TEST fill_black: RUNNING");
    for pixel in fb.iter_mut().take(WIDTH * HEIGHT) {
        *pixel = 0x00000000;
    }
    delay.delay_ms(500u32);
    pass("fill_black");

    // Test 11: Gradient fill (stress test - no crash = pass)
    defmt::info!("TEST gradient_fill: RUNNING");
    for frame in 0..10 {
        for row in 0..HEIGHT {
            let r = ((row * 255) / HEIGHT) as u32;
            let g = ((frame * 255) / 10) as u32;
            let b = 255 - r;
            let color = (r << 16) | (g << 8) | b;
            let start = row * WIDTH;
            for pixel in fb.iter_mut().take(WIDTH).skip(start) {
                *pixel = color;
            }
        }
        delay.delay_ms(100u32);
    }
    pass("gradient_fill");

    // Test 12: Rapid refresh (stability)
    defmt::info!("TEST rapid_refresh: RUNNING");
    for frame in 0..30 {
        let color = if frame % 2 == 0 {
            0x00FF0000
        } else {
            0x0000FF00
        };
        for pixel in fb.iter_mut().take(WIDTH * HEIGHT) {
            *pixel = color;
        }
        delay.delay_ms(33u32);
    }
    pass("rapid_refresh");

    // Final: display a nice pattern and hold
    defmt::info!("TEST continuous_display: RUNNING");
    let mut hue = 0u32;
    loop {
        for row in 0..HEIGHT {
            let r = (hue + row as u32 * 3) % 256;
            let g = (hue + row as u32 * 3 + 85) % 256;
            let b = (hue + row as u32 * 3 + 170) % 256;
            let color = (r << 16) | (g << 8) | b;
            let start = row * WIDTH;
            for pixel in fb.iter_mut().take(WIDTH).skip(start) {
                *pixel = color;
            }
        }
        hue = hue.wrapping_add(1);
        delay.delay_ms(50u32);

        // Only mark pass once we've done a few frames
        if hue == 60 {
            pass("continuous_display");

            let passed = PASSED.load(Ordering::Relaxed);
            let failed = FAILED.load(Ordering::Relaxed);
            let total = passed + failed;

            defmt::info!("=== LCD Test Summary ===");
            defmt::info!("SUMMARY: {}/{} passed", passed, total);

            if failed == 0 {
                defmt::info!("ALL TESTS PASSED");
            } else {
                defmt::error!("FAILED: {} tests failed", failed);
            }
        }
    }
}

#[exception]
unsafe fn HardFault(ef: &cortex_m_rt::ExceptionFrame) -> ! {
    panic!("HardFault at {:#?}", ef);
}

#[exception]
unsafe fn DefaultHandler(irqn: i16) {
    panic!("Unhandled exception (IRQn = {})", irqn);
}
