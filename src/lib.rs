#![no_std]
#![allow(non_camel_case_types)]

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
