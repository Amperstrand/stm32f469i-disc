//! LCD display initialization for the STM32F469I-DISCO board.
//!
//! Provides the complete DSI + LTDC bring-up sequence supporting both
//! board revisions:
//! - B08 and later: NT35510 LCD controller
//! - B07 and earlier: OTM8009A LCD controller
//!
//! The panel is auto-detected at runtime via DSI probe reads.
//!
//! # Usage
//!
//! ```no_run
//! let (mut display_ctrl, _controller, _orientation) = lcd::init_display_full(
//!     dp.DSI, dp.LTDC, dp.DMA2D,
//!     &mut rcc, &mut delay,
//!     lcd::BoardHint::Unknown,
//!     lcd::DisplayOrientation::Portrait,
//! );
//! display_ctrl.config_layer(Layer::L1, buffer, PixelFormat::RGB565);
//! display_ctrl.enable_layer(Layer::L1);
//! display_ctrl.reload();
//! ```

// Based on STM32CubeF4 BSP LCD driver (STMicroelectronics, BSD-3-Clause)

use crate::hal::{
    dsi::{
        ColorCoding, DsiChannel, DsiCmdModeTransmissionKind, DsiConfig, DsiHost, DsiInterrupts,
        DsiMode, DsiPhyTimers, DsiPllConfig, DsiVideoMode, LaneCount,
    },
    ltdc::{DisplayConfig, DisplayController, PixelFormat},
    pac::{DMA2D, DSI, LTDC},
    prelude::*,
    rcc::Rcc,
    time::Hertz,
};
#[cfg(feature = "framebuffer")]
use crate::hal::{
    ltdc::{Layer, LtdcFramebuffer},
    pac,
    timer::SysDelay,
};
#[cfg(feature = "framebuffer")]
use crate::sdram::{self, SdramRemainders};

#[cfg(feature = "framebuffer")]
use embedded_graphics_core::{
    draw_target::DrawTarget,
    pixelcolor::{Rgb565, RgbColor},
};
use embedded_hal::delay::DelayNs;
use embedded_hal_02::blocking::delay::{DelayMs, DelayUs};
use nt35510::Nt35510;
use otm8009a::{Otm8009A, Otm8009AConfig};

/// Panel physical width in pixels (portrait orientation).
pub const PANEL_WIDTH: u16 = 480;
/// Panel physical height in pixels (portrait orientation).
pub const PANEL_HEIGHT: u16 = 800;

/// Display orientation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum DisplayOrientation {
    /// Portrait: 480 pixels wide, 800 pixels tall (native panel orientation).
    Portrait,
    /// Landscape: 800 pixels wide, 480 pixels tall.
    Landscape,
}

impl DisplayOrientation {
    pub const fn width(self) -> u16 {
        match self {
            DisplayOrientation::Portrait => PANEL_WIDTH,
            DisplayOrientation::Landscape => PANEL_HEIGHT,
        }
    }

    pub const fn height(self) -> u16 {
        match self {
            DisplayOrientation::Portrait => PANEL_HEIGHT,
            DisplayOrientation::Landscape => PANEL_WIDTH,
        }
    }

    pub const fn fb_size(self) -> usize {
        (self.width() as usize) * (self.height() as usize)
    }
}

/// NT35510 display timing (B08 revision, portrait).
pub const NT35510_DISPLAY_CONFIG: DisplayConfig = DisplayConfig {
    active_width: PANEL_WIDTH,
    active_height: PANEL_HEIGHT,
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

/// NT35510 display timing (B08 revision, landscape).
pub const NT35510_DISPLAY_CONFIG_LANDSCAPE: DisplayConfig = DisplayConfig {
    active_width: PANEL_HEIGHT,
    active_height: PANEL_WIDTH,
    h_back_porch: 15,
    h_front_porch: 16,
    v_back_porch: 34,
    v_front_porch: 34,
    h_sync: 1,
    v_sync: 2,
    frame_rate: 60,
    h_sync_pol: true,
    v_sync_pol: true,
    no_data_enable_pol: false,
    pixel_clock_pol: true,
};

/// OTM8009A display timing (B07 and earlier revisions, portrait).
pub const OTM8009A_DISPLAY_CONFIG: DisplayConfig = DisplayConfig {
    active_width: PANEL_WIDTH,
    active_height: PANEL_HEIGHT,
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

/// OTM8009A display timing (B07 and earlier revisions, landscape).
pub const OTM8009A_DISPLAY_CONFIG_LANDSCAPE: DisplayConfig = DisplayConfig {
    active_width: PANEL_HEIGHT,
    active_height: PANEL_WIDTH,
    h_back_porch: 15,
    h_front_porch: 16,
    v_back_porch: 34,
    v_front_porch: 34,
    h_sync: 1,
    v_sync: 2,
    frame_rate: 60,
    h_sync_pol: true,
    v_sync_pol: true,
    no_data_enable_pol: false,
    pixel_clock_pol: true,
};

/// Default display config (portrait, works for both panel types).
pub const DISPLAY_CONFIG: DisplayConfig = NT35510_DISPLAY_CONFIG;

/// Backwards-compatible aliases.
#[deprecated = "Use DisplayOrientation::Portrait and PANEL_WIDTH/PANEL_HEIGHT instead"]
pub const WIDTH: u16 = PANEL_WIDTH;
#[deprecated = "Use DisplayOrientation::Portrait and PANEL_WIDTH/PANEL_HEIGHT instead"]
pub const HEIGHT: u16 = PANEL_HEIGHT;
#[deprecated = "Use DisplayOrientation::fb_size() instead"]
pub const FB_SIZE: usize = DisplayOrientation::Portrait.fb_size();

/// Detected / selected LCD controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LcdController {
    Nt35510,
    Otm8009a,
}

impl LcdController {
    /// Return the LTDC timing configuration for this controller.
    pub fn display_config(self, orientation: DisplayOrientation) -> DisplayConfig {
        match (self, orientation) {
            (LcdController::Nt35510, DisplayOrientation::Portrait) => NT35510_DISPLAY_CONFIG,
            (LcdController::Nt35510, DisplayOrientation::Landscape) => {
                NT35510_DISPLAY_CONFIG_LANDSCAPE
            }
            (LcdController::Otm8009a, DisplayOrientation::Portrait) => OTM8009A_DISPLAY_CONFIG,
            (LcdController::Otm8009a, DisplayOrientation::Landscape) => {
                OTM8009A_DISPLAY_CONFIG_LANDSCAPE
            }
        }
    }
}

/// Hint about board revision from external probes (e.g. touch controller I2C).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BoardHint {
    /// FT6X06 at 0x38 found — likely NT35510 (newer revision).
    NewRevisionLikely,
    /// Legacy touch at 0x2A found — likely OTM8009A (older revision).
    LegacyRevisionLikely,
    /// No reliable hint available.
    Unknown,
    /// Skip probe entirely — force NT35510 (B08 board).
    /// Use when DSI probe reads are known to be unreliable.
    ForceNt35510,
}

/// Detect which LCD controller is connected via DSI probe.
///
/// Uses 3 probe retries with delays. Tracks read/write errors and mismatches.
/// Uses the board hint to inform the fallback decision.
pub fn detect_lcd_controller(
    dsi_host: &mut DsiHost,
    delay: &mut (impl DelayUs<u32> + DelayMs<u32> + DelayNs),
    board_hint: BoardHint,
) -> LcdController {
    if let BoardHint::ForceNt35510 = board_hint {
        #[cfg(feature = "defmt")]
        defmt::info!("NT35510 forced — skipping probe");
        return LcdController::Nt35510;
    }

    const PROBE_RETRIES: u8 = 3;
    embedded_hal_02::blocking::delay::DelayUs::<u32>::delay_us(delay, 20_000u32);

    let mut nt35510 = Nt35510::new();
    let mut mismatch_count = 0u8;
    let mut first_mismatch_id: Option<u8> = None;
    let mut consistent_mismatch = true;
    let mut read_error_count = 0u8;
    let mut write_error_count = 0u8;

    for attempt in 1..=PROBE_RETRIES {
        #[cfg(not(feature = "defmt"))]
        let _ = attempt;
        match nt35510.probe(dsi_host, delay) {
            Ok(_) => {
                #[cfg(feature = "defmt")]
                defmt::info!("NT35510 (B08) detected on attempt {}", attempt);
                return LcdController::Nt35510;
            }
            Err(nt35510::Error::DsiRead) => {
                read_error_count = read_error_count.saturating_add(1);
                #[cfg(feature = "defmt")]
                defmt::warn!("NT35510 probe attempt {} failed: DSI read error", attempt);
            }
            Err(nt35510::Error::DsiWrite) => {
                write_error_count = write_error_count.saturating_add(1);
                #[cfg(feature = "defmt")]
                defmt::warn!("NT35510 probe attempt {} failed: DSI write error", attempt);
            }
            Err(nt35510::Error::ProbeMismatch(id)) => {
                mismatch_count = mismatch_count.saturating_add(1);
                match first_mismatch_id {
                    None => first_mismatch_id = Some(id),
                    Some(first) if first != id => consistent_mismatch = false,
                    Some(_) => {}
                }
                #[cfg(feature = "defmt")]
                defmt::info!(
                    "NT35510 probe attempt {} mismatch: RDID2=0x{:02x}",
                    attempt,
                    id
                );
            }
            Err(nt35510::Error::InvalidDimensions) => {
                #[cfg(feature = "defmt")]
                defmt::warn!(
                    "NT35510 probe attempt {} failed: invalid dimensions",
                    attempt
                );
            }
        }
        embedded_hal_02::blocking::delay::DelayUs::<u32>::delay_us(delay, 5_000u32);
    }

    let fallback_to_otm = match board_hint {
        BoardHint::ForceNt35510 => unreachable!("handled above"),
        BoardHint::LegacyRevisionLikely => mismatch_count >= 1 && consistent_mismatch,
        BoardHint::NewRevisionLikely => mismatch_count >= PROBE_RETRIES && consistent_mismatch,
        BoardHint::Unknown => mismatch_count >= 2 && consistent_mismatch,
    };

    if fallback_to_otm {
        #[cfg(feature = "defmt")]
        {
            let mismatch_id = first_mismatch_id.unwrap_or(0xFF);
            defmt::info!(
                "Consistent non-NT35510 response (id=0x{:02x}, count={}); falling back to OTM8009A",
                mismatch_id,
                mismatch_count
            );
        }
        LcdController::Otm8009a
    } else {
        #[cfg(feature = "defmt")]
        defmt::warn!(
            "Probe inconclusive (mismatch={}, read_err={}, write_err={}); defaulting to NT35510",
            mismatch_count,
            read_error_count,
            write_error_count
        );
        LcdController::Nt35510
    }
}

/// Initialize the DSI host with F469-DISCO settings.
///
/// After calling this, wait 20ms before any panel communication.
/// Prefer [`init_dsi_with_delay`] which includes the delay.
pub fn init_dsi(dsi: DSI, rcc: &mut Rcc, display_config: DisplayConfig) -> DsiHost {
    let hse_freq = 8.MHz();
    let ltdc_freq = 27_429.kHz();
    // VCO = (8MHz HSE / 2 IDF) * 2 * 125 = 1000MHz
    // 1000MHz VCO / (2 * 1 ODF * 8) = 62.5MHz
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
        color_coding_host: ColorCoding::SixteenBitsConfig1,
        color_coding_wrapper: ColorCoding::SixteenBitsConfig1,
        lp_size: 64,
        vlp_size: 64,
    };

    #[cfg(feature = "defmt")]
    defmt::info!("Initializing DSI...");
    let mut dsi_host = DsiHost::init(dsi_pll_config, display_config, dsi_config, dsi, rcc).unwrap();

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

    dsi_host
}

/// Initialize DSI host and wait for panel link to settle (20ms).
pub fn init_dsi_with_delay(dsi: DSI, rcc: &mut Rcc, delay: &mut impl DelayMs<u32>) -> DsiHost {
    let dsi_host = init_dsi(dsi, rcc, DISPLAY_CONFIG);
    delay.delay_ms(20u32);
    dsi_host
}

/// Detect and initialize the LCD panel, then switch DSI to high-speed mode.
pub fn init_panel(
    dsi_host: &mut DsiHost,
    delay: &mut (impl DelayUs<u32> + DelayMs<u32> + DelayNs),
    board_hint: BoardHint,
) -> LcdController {
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInLowPower);
    dsi_host.force_rx_low_power(true);

    let controller = detect_lcd_controller(dsi_host, delay, board_hint);

    match controller {
        LcdController::Nt35510 => {
            #[cfg(feature = "defmt")]
            defmt::info!("Initializing NT35510 (B08 revision)...");
            let mut panel = Nt35510::new();
            panel
                .init_rgb565(
                    dsi_host,
                    delay,
                    nt35510::Mode::Portrait,
                    nt35510::ColorMap::Rgb,
                )
                .unwrap();
        }
        LcdController::Otm8009a => {
            #[cfg(feature = "defmt")]
            defmt::info!("Initializing OTM8009A (B07 and earlier)...");
            let otm_config = Otm8009AConfig {
                frame_rate: otm8009a::FrameRate::_60Hz,
                mode: otm8009a::Mode::Portrait,
                color_map: otm8009a::ColorMap::Rgb,
                cols: PANEL_WIDTH,
                rows: PANEL_HEIGHT,
            };
            let mut otm = Otm8009A::new();
            otm.init(dsi_host, otm_config, delay).unwrap();
        }
    }

    dsi_host.force_rx_low_power(false);
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
    #[cfg(feature = "defmt")]
    defmt::info!("Panel initialized, DSI in high-speed mode");
    controller
}

/// Create the LTDC display controller for RGB565.
///
/// Configures PLLSAI/R to generate the LTDC pixel clock from HSE.
/// On STM32F469 the LTDC pixel clock is always sourced from PLLSAI_R / PLLSAIDIVR,
/// even in DSI mode — there is no mux to select the DSI clock.
pub fn init_ltdc_rgb565(
    ltdc: LTDC,
    dma2d: DMA2D,
    controller: LcdController,
    orientation: DisplayOrientation,
    hse_freq: Hertz,
) -> DisplayController<u16> {
    DisplayController::<u16>::new(
        ltdc,
        dma2d,
        None,
        PixelFormat::RGB565,
        controller.display_config(orientation),
        Some(hse_freq),
    )
}

/// Create the LTDC display controller for ARGB8888.
///
/// Configures PLLSAI/R to generate the LTDC pixel clock from HSE.
/// On STM32F469 the LTDC pixel clock is always sourced from PLLSAI_R / PLLSAIDIVR,
/// even in DSI mode — there is no mux to select the DSI clock.
pub fn init_ltdc_argb8888(
    ltdc: LTDC,
    dma2d: DMA2D,
    controller: LcdController,
    orientation: DisplayOrientation,
    hse_freq: Hertz,
) -> DisplayController<u32> {
    DisplayController::<u32>::new(
        ltdc,
        dma2d,
        None,
        PixelFormat::ARGB8888,
        controller.display_config(orientation),
        Some(hse_freq),
    )
}

/// Full display initialization following the proven lcd-test sequence.
///
/// Handles the complete init sequence in the correct order:
/// 1. DSI host init
/// 2. 20ms delay for panel link settle
/// 3. LCD controller detection
/// 4. LTDC initialization (before panel init — this is critical)
/// 5. Panel initialization
/// 6. Switch DSI to high-speed mode
///
/// Returns `(DisplayController, LcdController, DisplayOrientation)`.
pub fn init_display_full(
    dsi: DSI,
    ltdc: LTDC,
    dma2d: DMA2D,
    rcc: &mut Rcc,
    delay: &mut (impl DelayUs<u32> + DelayMs<u32> + DelayNs),
    board_hint: BoardHint,
    orientation: DisplayOrientation,
) -> (DisplayController<u16>, LcdController, DisplayOrientation) {
    // Step 1: DSI host init
    let display_timing = LcdController::Nt35510.display_config(orientation);
    let mut dsi_host = init_dsi(dsi, rcc, display_timing);

    // Step 2: Critical delay for panel link
    embedded_hal_02::blocking::delay::DelayMs::<u32>::delay_ms(delay, 20u32);

    // Step 3: Detect LCD controller
    let controller = detect_lcd_controller(&mut dsi_host, delay, board_hint);
    #[cfg(feature = "defmt")]
    defmt::info!("Detected LCD controller: {:?}", controller);

    // Step 4: Initialize LTDC BEFORE panel init
    // PLLSAI/R must be configured even in DSI mode — the LTDC pixel clock on
    // STM32F469 is always sourced from PLLSAI_R / PLLSAIDIVR (no mux to DSI).
    let hse_freq = 8.MHz();
    let display_ctrl = DisplayController::<u16>::new(
        ltdc,
        dma2d,
        None,
        PixelFormat::RGB565,
        controller.display_config(orientation),
        Some(hse_freq),
    );

    // Step 5: Set command mode and init panel
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInLowPower);
    dsi_host.force_rx_low_power(true);

    match controller {
        LcdController::Nt35510 => {
            #[cfg(feature = "defmt")]
            defmt::info!("Initializing NT35510 (B08 revision)...");
            let mut panel = Nt35510::new();
            panel
                .init_rgb565(
                    &mut dsi_host,
                    delay,
                    nt35510::Mode::Portrait,
                    nt35510::ColorMap::Rgb,
                )
                .unwrap();
        }
        LcdController::Otm8009a => {
            #[cfg(feature = "defmt")]
            defmt::info!("Initializing OTM8009A (B07 and earlier)...");
            let otm_config = Otm8009AConfig {
                frame_rate: otm8009a::FrameRate::_60Hz,
                mode: otm8009a::Mode::Portrait,
                color_map: otm8009a::ColorMap::Rgb,
                cols: PANEL_WIDTH,
                rows: PANEL_HEIGHT,
            };
            let mut otm = Otm8009A::new();
            otm.init(&mut dsi_host, otm_config, delay).unwrap();
        }
    }

    // Step 6: Switch to high-speed mode
    dsi_host.force_rx_low_power(false);
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
    #[cfg(feature = "defmt")]
    defmt::info!("Display initialized successfully");

    (display_ctrl, controller, orientation)
}

#[allow(clippy::too_many_arguments)]
#[cfg(feature = "framebuffer")]
pub fn init_display_pipeline(
    fmc: pac::FMC,
    dsi: pac::DSI,
    ltdc: pac::LTDC,
    dma2d: pac::DMA2D,
    gpioc: stm32f4xx_hal::gpio::gpioc::Parts,
    gpiod: stm32f4xx_hal::gpio::gpiod::Parts,
    gpioe: stm32f4xx_hal::gpio::gpioe::Parts,
    gpiof: stm32f4xx_hal::gpio::gpiof::Parts,
    gpiog: stm32f4xx_hal::gpio::gpiog::Parts,
    gpioh: stm32f4xx_hal::gpio::gpioh::Parts,
    gpioi: stm32f4xx_hal::gpio::gpioi::Parts,
    rcc: &mut Rcc,
    delay: &mut SysDelay,
    orientation: DisplayOrientation,
) -> (LtdcFramebuffer<u16>, SdramRemainders) {
    let (sdram_pins, remainders, ph7) =
        sdram::split_sdram_pins(gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi);

    let mut lcd_reset = ph7.into_push_pull_output();
    lcd_reset.set_low();
    embedded_hal_02::blocking::delay::DelayMs::<u32>::delay_ms(delay, 20u32);
    lcd_reset.set_high();
    embedded_hal_02::blocking::delay::DelayMs::<u32>::delay_ms(delay, 10u32);

    let mut sdram = sdram::Sdram::new(fmc, sdram_pins, &rcc.clocks, delay);
    let buffer: &'static mut [u16] = sdram.subslice_mut(0, orientation.fb_size());
    let mut fb = LtdcFramebuffer::new(buffer, orientation.width(), orientation.height());
    fb.clear(Rgb565::BLACK).ok();
    let buffer = fb.into_inner();

    let (mut display_ctrl, _lcd_controller, _orientation) = init_display_full(
        dsi,
        ltdc,
        dma2d,
        rcc,
        delay,
        BoardHint::Unknown,
        orientation,
    );

    display_ctrl.config_layer(Layer::L1, buffer, PixelFormat::RGB565);
    display_ctrl.enable_layer(Layer::L1);
    display_ctrl.reload();

    let buffer = display_ctrl
        .layer_buffer_mut(Layer::L1)
        .expect("layer L1 buffer");
    let buffer: &'static mut [u16] = unsafe { core::mem::transmute(buffer) };

    (
        LtdcFramebuffer::new(buffer, orientation.width(), orientation.height()),
        remainders,
    )
}

#[cfg(feature = "framebuffer")]
/// Tear-free double framebuffer for the STM32F469I-DISCO display.
///
/// Holds two SDRAM-backed framebuffers and a `DisplayController` reference.
/// Drawing goes into the **back buffer**; calling [`swap()`](Self::swap) queues
/// an atomic buffer flip at the next vertical blanking period.
///
/// # Example
///
/// ```ignore
/// let mut dbl = lcd::DoubleFramebuffer::new(
///     &mut sdram, display_ctrl, DisplayOrientation::Portrait,
/// );
/// // draw into back buffer
/// dbl.back_buffer().fill(0x0000);
/// dbl.swap();
/// ```
pub struct DoubleFramebuffer {
    front: &'static mut [u16],
    back: &'static mut [u16],
    display_ctrl: DisplayController<u16>,
}

#[cfg(feature = "framebuffer")]
impl DoubleFramebuffer {
    /// Create a new double-buffered display pipeline.
    ///
    /// Allocates two framebuffers from SDRAM via `subslice_mut`, configures
    /// LTDC layer 1 with the front buffer, and enables the layer.
    ///
    /// The `sdram` instance is borrowed (not consumed) so the caller can
    /// continue allocating from it (e.g. for the heap).
    pub fn new(
        sdram: &mut sdram::Sdram,
        mut display_ctrl: DisplayController<u16>,
        orientation: DisplayOrientation,
    ) -> Self {
        let fb_size = orientation.fb_size();
        let fb1: &'static mut [u16] = sdram.subslice_mut(0, fb_size);
        let fb2: &'static mut [u16] = sdram.subslice_mut(fb_size * 2, fb_size);

        for px in fb1.iter_mut() {
            *px = 0;
        }
        for px in fb2.iter_mut() {
            *px = 0;
        }

        let fb1_addr = fb1.as_ptr() as u32;
        display_ctrl.config_layer(Layer::L1, fb1, PixelFormat::RGB565);
        display_ctrl.enable_layer(Layer::L1);
        display_ctrl.reload();

        // SAFETY: config_layer() takes ownership of the buffer and stores it
        // internally. We reconstruct a &'static mut from the known address
        // because the HAL's layer_buffer_mut() ties the borrow to &mut self,
        // preventing us from moving display_ctrl into Self. The buffer lives
        // for 'static in SDRAM and we are the sole owner.
        let fb1: &'static mut [u16] =
            unsafe { &mut *core::ptr::slice_from_raw_parts_mut(fb1_addr as *mut u16, fb_size) };

        Self {
            front: fb1,
            back: fb2,
            display_ctrl,
        }
    }

    /// Get mutable access to the back (draw) buffer.
    ///
    /// This buffer is not being scanned out by the display controller,
    /// so writes are safe from tearing.
    pub fn back_buffer(&mut self) -> &mut [u16] {
        &mut self.back
    }

    /// Execute a closure with a temporary `LtdcFramebuffer` wrapping the back
    /// buffer, then swap the buffers. This is the recommended way to render
    /// into the double framebuffer from code that expects `LcdcFramebuffer`.
    ///
    /// # Safety
    /// The caller must ensure `width * height` matches the buffer size used
    /// to create this `DoubleFramebuffer`.
    pub fn render_and_swap<F>(&mut self, width: u16, height: u16, f: F)
    where
        F: FnOnce(&mut crate::hal::ltdc::LtdcFramebuffer<u16>),
    {
        let back_ptr = self.back.as_mut_ptr();
        let back_len = self.back.len();
        let mut tmp_fb = unsafe {
            let static_buf: &'static mut [u16] =
                core::mem::transmute(core::slice::from_raw_parts_mut(back_ptr, back_len));
            crate::hal::ltdc::LtdcFramebuffer::new(static_buf, width, height)
        };
        f(&mut tmp_fb);
        defmt::trace!("render_and_swap: calling swap");
        self.swap();
    }

    /// Queue a buffer swap at the next vertical blanking period.
    ///
    /// After this call the current back buffer becomes the front buffer
    /// and vice versa. The LTDC hardware will atomically switch the
    /// framebuffer address during vblank, preventing tearing.
    ///
    /// Uses `DisplayController::swap_buffers()` which waits for any
    /// pending VBlank reload to complete before writing the new address,
    /// preventing the race condition that can interfere with USB DMA.
    pub fn swap(&mut self) {
        if let Err(e) = self
            .display_ctrl
            .swap_buffers(Layer::L1, self.back.as_ptr() as u32)
        {
            defmt::warn!("swap failed: {:?}", e);
            return;
        }
        core::mem::swap(&mut self.front, &mut self.back);
    }

    /// Get mutable access to the underlying `DisplayController`.
    pub fn display_controller(&mut self) -> &mut DisplayController<u16> {
        &mut self.display_ctrl
    }

    /// Get the current front (displayed) buffer.
    ///
    /// Writing to this buffer while it is being displayed will cause tearing.
    pub fn front_buffer(&mut self) -> &mut [u16] {
        &mut self.front
    }

    /// Consume the double framebuffer and return the currently displayed buffer.
    ///
    /// The `DisplayController` is dropped. Use this to downgrade to a single
    /// `LtdcFramebuffer` after double-buffered animation is complete.
    pub fn into_front_buffer(mut self) -> &'static mut [u16] {
        self.front
    }

    /// Consume the double framebuffer and return the front buffer and the
    /// `DisplayController` separately, allowing the controller to be used for
    /// further operations (e.g., periodic `swap_buffers()` calls).
    pub fn into_parts(mut self) -> (&'static mut [u16], DisplayController<u16>) {
        (self.front, self.display_ctrl)
    }
}
