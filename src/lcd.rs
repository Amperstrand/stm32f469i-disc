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
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb888, prelude::*};
use embedded_hal::delay::DelayNs;
use nt35510::{Nt35510, PanelTiming};
use otm8009a::{Otm8009A, Otm8009AConfig};

/// Panel physical width in pixels (portrait orientation).
#[deprecated = "Use nt35510::PANEL_WIDTH instead"]
pub const PANEL_WIDTH: u16 = nt35510::PANEL_WIDTH;
/// Panel physical height in pixels (portrait orientation).
#[deprecated = "Use nt35510::PANEL_HEIGHT instead"]
pub const PANEL_HEIGHT: u16 = nt35510::PANEL_HEIGHT;

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
    /// Width in pixels for this orientation.
    pub const fn width(self) -> u16 {
        match self {
            DisplayOrientation::Portrait => nt35510::PANEL_WIDTH,
            DisplayOrientation::Landscape => nt35510::PANEL_HEIGHT,
        }
    }

    /// Height in pixels for this orientation.
    pub const fn height(self) -> u16 {
        match self {
            DisplayOrientation::Portrait => nt35510::PANEL_HEIGHT,
            DisplayOrientation::Landscape => nt35510::PANEL_WIDTH,
        }
    }

    /// Framebuffer size in pixels (`width * height`).
    pub const fn fb_size(self) -> usize {
        (self.width() as usize) * (self.height() as usize)
    }

    /// Map to the corresponding NT35510 panel orientation.
    pub const fn nt35510_mode(self) -> nt35510::Mode {
        match self {
            DisplayOrientation::Portrait => nt35510::Mode::Portrait,
            DisplayOrientation::Landscape => nt35510::Mode::Landscape,
        }
    }

    /// Map to the corresponding OTM8009A panel orientation.
    pub const fn otm8009a_mode(self) -> otm8009a::Mode {
        match self {
            DisplayOrientation::Portrait => otm8009a::Mode::Portrait,
            DisplayOrientation::Landscape => otm8009a::Mode::Landscape,
        }
    }
}

/// NT35510 display timing (B08 revision, portrait).
pub const NT35510_DISPLAY_CONFIG: DisplayConfig = display_config_from_timing(
    nt35510::PANEL_WIDTH,
    nt35510::PANEL_HEIGHT,
    PanelTiming::STANDARD_PORTRAIT,
);

/// NT35510 display timing (B08 revision, landscape).
pub const NT35510_DISPLAY_CONFIG_LANDSCAPE: DisplayConfig = display_config_from_timing(
    nt35510::PANEL_HEIGHT,
    nt35510::PANEL_WIDTH,
    PanelTiming::STANDARD_LANDSCAPE,
);

/// OTM8009A display timing (B07 and earlier revisions, portrait).
pub const OTM8009A_DISPLAY_CONFIG: DisplayConfig = display_config_from_timing(
    nt35510::PANEL_WIDTH,
    nt35510::PANEL_HEIGHT,
    PanelTiming::STANDARD_PORTRAIT,
);

/// OTM8009A display timing (B07 and earlier revisions, landscape).
pub const OTM8009A_DISPLAY_CONFIG_LANDSCAPE: DisplayConfig = display_config_from_timing(
    nt35510::PANEL_HEIGHT,
    nt35510::PANEL_WIDTH,
    PanelTiming::STANDARD_LANDSCAPE,
);

/// Default display config (portrait, works for both panel types).
pub const DISPLAY_CONFIG: DisplayConfig = NT35510_DISPLAY_CONFIG;

const fn display_config_from_timing(
    active_width: u16,
    active_height: u16,
    timing: PanelTiming,
) -> DisplayConfig {
    DisplayConfig {
        active_width,
        active_height,
        h_back_porch: timing.h_back_porch,
        h_front_porch: timing.h_front_porch,
        v_back_porch: timing.v_back_porch,
        v_front_porch: timing.v_front_porch,
        h_sync: timing.h_sync,
        v_sync: timing.v_sync,
        frame_rate: timing.frame_rate,
        h_sync_pol: true,
        v_sync_pol: true,
        no_data_enable_pol: false,
        pixel_clock_pol: true,
    }
}

/// Detected / selected LCD controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum LcdController {
    /// NT35510 panel (B08 revision and later).
    Nt35510,
    /// OTM8009A panel (B07 and earlier revisions).
    Otm8009A,
}

impl LcdController {
    /// Return the LTDC timing configuration for this controller.
    pub fn display_config(self, orientation: DisplayOrientation) -> DisplayConfig {
        let timing = PanelTiming::for_mode(orientation.nt35510_mode());
        let (active_width, active_height) = match orientation {
            DisplayOrientation::Portrait => (nt35510::PANEL_WIDTH, nt35510::PANEL_HEIGHT),
            DisplayOrientation::Landscape => (nt35510::PANEL_HEIGHT, nt35510::PANEL_WIDTH),
        };

        match self {
            LcdController::Nt35510 | LcdController::Otm8009A => {
                display_config_from_timing(active_width, active_height, timing)
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
    /// Auto-detect by probing I2C1 for FT6X06 touch controller before DSI init.
    /// FT6X06 at 0x38 indicates NT35510 (B08); no response or different chip
    /// suggests OTM8009A (B07). Falls back to DSI probe logic for confirmation.
    /// Requires the `touch` feature.
    Auto,
}

/// Probe I2C1 for the FT6X06 touch controller to determine board revision hint.
///
/// This is a lightweight pre-DSI check that runs before the display pipeline
/// is initialized. On STM32F469I-DISCO:
/// - **B08+ (NT35510 panel)**: FT6X06 at I2C address 0x38 (PB8=SDA, PB9=SCL)
/// - **B07 and earlier (OTM8009A panel)**: No FT6X06; touch uses a different chip
///
/// Note: PH7 (LCD reset) must be toggled before calling this to power on
/// the touch controller's I2C bus. Without this, FT6X06 will NACK all transactions.
///
/// Requires the `touch` feature. The function is defined inside a
/// `#[cfg(feature = "touch")]` inline module to prevent the compiler from
/// resolving `touch::FT6X06_I2C_ADDR` when the feature is disabled (Rust
/// resolves paths before applying `#[cfg]` on items, causing spurious errors).
///
/// # How other projects handle this
/// - **ST's official BSP** (`BSP_DISCO_F469NI`): Compile-time `#define`
///   to select the LCD controller. No runtime detection.
/// - **specter-diy**: Does no board revision detection. Assumes whatever panel
///   is present works with their display init.
/// - **lvgl/lv_porting_stm32**: Compile-time board config, similar to ST.
/// - **mipidsi (EPD driver crate)**: Reads panel ID via MIPI DCS commands
///   (same approach as our DSI probe, subject to the same read reliability issues).
///
/// # Limitations
/// - Cannot distinguish MCU silicon revision from board revision (DBGMCU_IDCODE
///   and UID are MCU-specific, not board-specific).
/// - Touch controller presence is correlated with board revision but not
///   definitive — a board could have a swapped touch controller.
/// - I2C probe requires GPIO and RCC to be configured, which must be done
///   before calling this function.
#[cfg(feature = "touch")]
mod board_probe {
    use super::BoardHint;

    const FT6X06_I2C_ADDR: u8 = 0x38;

    /// Probe I2C for the FT6X06 touch controller to determine board revision hint.
    /// Returns [`BoardHint::NewRevisionLikely`] if FT6X06 responds, or
    /// [`BoardHint::LegacyRevisionLikely`] if it does not.
    pub fn probe(i2c: &mut impl embedded_hal::i2c::I2c) -> BoardHint {
        let mut buf = [0u8; 1];
        match i2c.write_read(FT6X06_I2C_ADDR, &[0xA8], &mut buf) {
            Ok(()) => {
                #[cfg(feature = "defmt")]
                defmt::info!(
                    "I2C probe: FT6X06 found at 0x{:02x} (chip_id=0x{:02x}) — likely B08/NT35510",
                    FT6X06_I2C_ADDR,
                    buf[0]
                );
                BoardHint::NewRevisionLikely
            }
            Err(_) => {
                #[cfg(feature = "defmt")]
                defmt::info!(
                    "I2C probe: no FT6X06 at 0x{:02x} — likely B07/OTM8009A or no touch panel",
                    FT6X06_I2C_ADDR
                );
                BoardHint::LegacyRevisionLikely
            }
        }
    }
}

/// Re-export for convenience.
#[cfg(feature = "touch")]
pub use board_probe::probe as probe_board_revision;

/// Detect which LCD controller is connected via DSI probe.
///
/// Uses 3 probe retries with delays. Tracks read/write errors and mismatches.
/// Uses the board hint to inform the fallback decision.
pub fn detect_lcd_controller(
    dsi_host: &mut DsiHost,
    delay: &mut impl DelayNs,
    board_hint: BoardHint,
) -> LcdController {
    if let BoardHint::ForceNt35510 = board_hint {
        #[cfg(feature = "defmt")]
        defmt::info!("NT35510 forced — skipping probe");
        return LcdController::Nt35510;
    }

    const PROBE_RETRIES: u8 = 3;
    delay.delay_us(20_000);

    let mut nt35510 = Nt35510::new();
    let mut mismatch_count = 0u8;
    let mut first_mismatch_id: Option<u8> = None;
    let mut consistent_mismatch = true;
    let mut read_error_count = 0u8;
    let mut write_error_count = 0u8;

    for attempt in 1..=PROBE_RETRIES {
        #[cfg(not(feature = "defmt"))]
        let _ = attempt;
        match nt35510.probe(dsi_host) {
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
        delay.delay_us(5_000);
    }

    let fallback_to_otm = match board_hint {
        BoardHint::ForceNt35510 => unreachable!("handled above"),
        BoardHint::LegacyRevisionLikely => mismatch_count >= 1 && consistent_mismatch,
        BoardHint::NewRevisionLikely => mismatch_count >= PROBE_RETRIES && consistent_mismatch,
        BoardHint::Unknown => mismatch_count >= 2 && consistent_mismatch,
        BoardHint::Auto => mismatch_count >= 2 && consistent_mismatch,
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
        LcdController::Otm8009A
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
pub fn init_dsi(
    dsi: DSI,
    rcc: &mut Rcc,
    display_config: DisplayConfig,
    color_coding: ColorCoding,
    ltdc_freq: Hertz,
) -> DsiHost {
    let hse_freq = 8.MHz();
    // VCO = (8MHz HSE / 2 IDF) * 2 * 125 = 1000MHz
    // 1000MHz VCO / (2 * 1 ODF * 8) = 62.5MHz
    // SAFETY: PLL parameters (NDIV=125, IDF=2, ODF=0, REG=4) produce a DSI bit clock
    // of ~312.5 MHz (VCO = (8 MHz / IDF) * 2 * NDIV = 1000 MHz; DSI = 1000 / (2 * (ODF+1) * REG) = 62.5 MHz byte clock * 5 = 312.5 MHz).
    // These values are board-specific and verified on the STM32F469I-DISCO.
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
        color_coding_host: color_coding,
        color_coding_wrapper: color_coding,
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
    // CR.EN deferred until after LTDC init — matches embassy pattern where
    // DSI host gets exactly one 0→1 transition after LTDC is running.
    // ForceNt35510 skips DSI reads, so no CR.EN needed for detection.
    dsi_host.enable_bus_turn_around();

    dsi_host
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
    delay: &mut impl DelayNs,
    board_hint: BoardHint,
    orientation: DisplayOrientation,
) -> (DisplayController<u16>, LcdController, DisplayOrientation) {
    #[cfg(feature = "defmt")]
    defmt::info!(
        "[init_display_full] starting, hint={:?}, orientation={:?}",
        board_hint,
        orientation
    );

    // Step 1: DSI host init
    let display_timing = LcdController::Nt35510.display_config(orientation);
    let mut dsi_host = init_dsi(
        dsi,
        rcc,
        display_timing,
        ColorCoding::SixteenBitsConfig1,
        27_429.kHz(),
    );
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full] step 1: DSI host initialized");

    // Step 2: Critical delay for panel link
    delay.delay_ms(20);

    // Step 3: Detect LCD controller
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full] step 2: probing LCD controller...");
    let controller = detect_lcd_controller(&mut dsi_host, delay, board_hint);
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full] step 2: detected {:?}", controller);

    // Stop DSI host after panel detection. ST BSP stops before reconfiguring
    // for video mode: HAL_DSI_Stop() → config → HAL_LTDC_Init() → HAL_DSI_Start().
    dsi_host.stop();

    // Step 4: Initialize LTDC BEFORE panel init
    // PLLSAI/R must be configured even in DSI mode — the LTDC pixel clock on
    // STM32F469 is always sourced from PLLSAI_R / PLLSAIDIVR (no mux to DSI).
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full] step 3: initializing LTDC (RGB565)...");
    let hse_freq = 8.MHz();
    let display_ctrl = DisplayController::<u16>::new(
        ltdc,
        dma2d,
        None,
        PixelFormat::RGB565,
        controller.display_config(orientation),
        Some(hse_freq),
    );
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full] step 3: LTDC initialized");

    // Start DSI (host + wrapper) AFTER LTDC — ST BSP ordering.
    dsi_host.start();

    // Step 5: Set command mode and init panel
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full] step 4: setting DSI command mode (low-power RX)");
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInLowPower);
    dsi_host.force_rx_low_power(true);

    match controller {
        LcdController::Nt35510 => {
            #[cfg(feature = "defmt")]
            defmt::info!("[init_display_full] step 5: initializing NT35510 (B08 revision)...");
            let mut panel = Nt35510::new();
            panel
                .init_rgb565(
                    &mut dsi_host,
                    delay,
                    orientation.nt35510_mode(),
                    nt35510::ColorMap::Rgb,
                )
                .unwrap();
            #[cfg(feature = "defmt")]
            defmt::info!("[init_display_full] step 5: NT35510 init complete");
        }
        LcdController::Otm8009A => {
            #[cfg(feature = "defmt")]
            defmt::info!("[init_display_full] step 5: initializing OTM8009A (B07 and earlier)...");
            let otm_config = Otm8009AConfig {
                frame_rate: otm8009a::FrameRate::_60Hz,
                mode: orientation.otm8009a_mode(),
                color_map: otm8009a::ColorMap::Rgb,
                cols: nt35510::PANEL_WIDTH,
                rows: nt35510::PANEL_HEIGHT,
            };
            let mut otm = Otm8009A::new();
            otm.init(&mut dsi_host, otm_config, delay).unwrap();
            #[cfg(feature = "defmt")]
            defmt::info!("[init_display_full] step 5: OTM8009A init complete");
        }
    }

    // Step 6: Switch to high-speed mode
    dsi_host.force_rx_low_power(false);
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
    #[cfg(feature = "defmt")]
    defmt::info!(
        "[init_display_full] step 6: DSI in high-speed mode, controller={:?}",
        controller
    );

    (display_ctrl, controller, orientation)
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
pub fn init_display_full_argb8888(
    dsi: DSI,
    ltdc: LTDC,
    dma2d: DMA2D,
    rcc: &mut Rcc,
    delay: &mut impl DelayNs,
    board_hint: BoardHint,
    orientation: DisplayOrientation,
) -> (DisplayController<u32>, LcdController, DisplayOrientation) {
    #[cfg(feature = "defmt")]
    defmt::info!(
        "[init_display_full_argb8888] starting, hint={:?}, orientation={:?}",
        board_hint,
        orientation
    );

    let display_timing = LcdController::Nt35510.display_config(orientation);
    let mut dsi_host = init_dsi(
        dsi,
        rcc,
        display_timing,
        ColorCoding::TwentyFourBits,
        27_429.kHz(),
    );
    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full_argb8888] step 1: DSI host initialized");

    delay.delay_ms(20);

    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full_argb8888] step 2: probing LCD controller...");
    let controller = detect_lcd_controller(&mut dsi_host, delay, board_hint);
    #[cfg(feature = "defmt")]
    defmt::info!(
        "[init_display_full_argb8888] step 2: detected {:?}",
        controller
    );

    // Stop DSI host before LTDC init — matches init_display_full() (RGB565) path.
    dsi_host.stop();

    let hse_freq = 8.MHz();
    let display_ctrl = DisplayController::<u32>::new(
        ltdc,
        dma2d,
        None,
        PixelFormat::ARGB8888,
        controller.display_config(orientation),
        Some(hse_freq),
    );

    // PLLSAI pixel clock fix: float search produces PLLN=192 (13714 kHz) instead
    // of the required PLLN=384 (27429 kHz). Override here to match embassy reference.
    // Verified by hardware register dump (T1): PLLN=192 with PLLM=8, PLLR=7, DIVR=2.
    // ref: embassy-stm32f469i-disco display.rs LTDC_PIXEL_CLK_KHZ=27429 (PLLN=384/PLLR=7/DIVR=2)
    // Math: pixel_clock = 8MHz * 384 / (8 * 7 * 2) = 27,429 kHz
    // SAFETY: PLLSAI is not in use by other peripherals at this point; display is not active.
    {
        let rcc = unsafe { &*crate::hal::pac::RCC::ptr() };
        // Stop PLLSAI before reconfiguring (RM0386 section 6.3.26)
        rcc.cr().modify(|_, w| w.pllsaion().off());
        while rcc.cr().read().pllsairdy().is_ready() {}
        // Override PLLN to 384 using .modify() to preserve PLLSAI P (USB 48MHz) and Q fields
        rcc.pllsaicfgr()
            .modify(|_, w| unsafe { w.pllsain().bits(384) });
        // Restart PLLSAI and wait for lock
        rcc.cr().modify(|_, w| w.pllsaion().on());
        while rcc.cr().read().pllsairdy().is_not_ready() {}
    }

    dsi_host.start();

    #[cfg(feature = "defmt")]
    defmt::info!("[init_display_full_argb8888] step 4: setting DSI command mode (low-power RX)");
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInLowPower);
    dsi_host.force_rx_low_power(true);

    match controller {
        LcdController::Nt35510 => {
            #[cfg(feature = "defmt")]
            defmt::info!(
                "[init_display_full_argb8888] step 5: initializing NT35510 (B08 revision)..."
            );
            let mut panel = Nt35510::new();
            panel
                .init_with_config(
                    &mut dsi_host,
                    delay,
                    nt35510::Nt35510Config {
                        mode: orientation.nt35510_mode(),
                        color_map: nt35510::ColorMap::Rgb,
                        color_format: nt35510::ColorFormat::Rgb888,
                        cols: nt35510::PANEL_WIDTH,
                        rows: nt35510::PANEL_HEIGHT,
                    },
                )
                .unwrap();
            #[cfg(feature = "defmt")]
            defmt::info!("[init_display_full_argb8888] step 5: NT35510 init complete");
        }
        LcdController::Otm8009A => {
            #[cfg(feature = "defmt")]
            defmt::info!(
                "[init_display_full_argb8888] step 5: initializing OTM8009A (B07 and earlier)..."
            );
            let otm_config = Otm8009AConfig {
                frame_rate: otm8009a::FrameRate::_60Hz,
                mode: orientation.otm8009a_mode(),
                color_map: otm8009a::ColorMap::Rgb,
                cols: nt35510::PANEL_WIDTH,
                rows: nt35510::PANEL_HEIGHT,
            };
            let mut otm = Otm8009A::new();
            otm.init(&mut dsi_host, otm_config, delay).unwrap();
            #[cfg(feature = "defmt")]
            defmt::info!("[init_display_full_argb8888] step 5: OTM8009A init complete");
        }
    }

    dsi_host.force_rx_low_power(false);
    dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
    #[cfg(feature = "defmt")]
    defmt::info!(
        "[init_display_full_argb8888] step 6: DSI in high-speed mode, controller={:?}",
        controller
    );

    (display_ctrl, controller, orientation)
}

#[cfg(feature = "framebuffer")]
/// A view into an ARGB8888 framebuffer that implements [`DrawTarget`].
///
/// Provides drawing operations on a `u32` slice buffer (ARGB8888 format).
pub struct FramebufferView<'a> {
    buffer: &'a mut [u32],
    width: usize,
    height: usize,
}

#[cfg(feature = "framebuffer")]
impl<'a> FramebufferView<'a> {
    /// Create a new framebuffer view from a raw buffer slice.
    ///
    /// The buffer must contain at least `width * height` pixels.
    pub fn new(buffer: &'a mut [u32], width: u32, height: u32) -> Self {
        Self {
            buffer,
            width: width as usize,
            height: height as usize,
        }
    }

    fn encode(color: Rgb888) -> u32 {
        0xFF00_0000 | ((color.r() as u32) << 16) | ((color.g() as u32) << 8) | (color.b() as u32)
    }

    /// Fill the entire framebuffer with a solid color.
    pub fn clear(&mut self, color: Rgb888) {
        let raw = Self::encode(color);
        for pixel in self.buffer.iter_mut() {
            *pixel = raw;
        }
    }
}

#[cfg(feature = "framebuffer")]
impl<'a> DrawTarget for FramebufferView<'a> {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            let x = pixel.0.x as usize;
            let y = pixel.0.y as usize;
            if x < self.width && y < self.height {
                self.buffer[y * self.width + x] = Self::encode(pixel.1);
            }
        }
        Ok(())
    }

    fn fill_contiguous<I>(
        &mut self,
        area: &embedded_graphics::primitives::Rectangle,
        color: I,
    ) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        let top = area.top_left.y.max(0) as usize;
        let bottom = (area.top_left.y + area.size.height as i32).min(self.height as i32) as usize;
        let left = area.top_left.x.max(0) as usize;
        let right = (area.top_left.x + area.size.width as i32).min(self.width as i32) as usize;

        let flat_color = color.into_iter().next().unwrap_or(Rgb888::BLACK);
        let raw = Self::encode(flat_color);

        for y in top..bottom {
            for x in left..right {
                self.buffer[y * self.width + x] = raw;
            }
        }
        Ok(())
    }

    fn clear(&mut self, color: Self::Color) -> Result<(), Self::Error> {
        self.clear(color);
        Ok(())
    }
}

#[cfg(feature = "framebuffer")]
impl<'a> OriginDimensions for FramebufferView<'a> {
    fn size(&self) -> Size {
        Size::new(self.width as u32, self.height as u32)
    }
}
