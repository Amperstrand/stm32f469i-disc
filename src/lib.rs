#![no_std]
#![warn(missing_docs)]

//! Board support crate for the STM32F469I-DISCO Discovery kit.
//!
//! An async version using Embassy is available at
//! [embassy-stm32f469i-disco](https://github.com/Amperstrand/embassy-stm32f469i-disco).
//! Plan is to upstream fixes and improvements to stm32f4xx-hal and related
//! crates once testing is complete.
//!
//! Provides drivers and initialization helpers for the on-board peripherals:
//! SDRAM, DSI/LTDC display, FT6X06 touch, USB OTG FS, LEDs, GPIO, SDIO.

pub use stm32f4xx_hal as hal;

pub use crate::hal::pac::interrupt::*;
pub use crate::hal::pac::Interrupt;
pub use crate::hal::pac::Peripherals;

pub mod button;
pub mod lcd;
pub mod led;
pub mod sdio;
pub mod sdram;
#[cfg(feature = "touch")]
pub mod touch;
#[cfg(feature = "usb_fs")]
pub mod usb;

/// HSE crystal frequency on the STM32F469I-DISCO board (8 MHz).
pub const HSE_FREQ_MHZ: u32 = 8;
