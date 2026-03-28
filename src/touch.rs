//! FT6X06 touch controller setup for STM32F469I-DISCO board
//!
//! Provides convenient initialization for the FT6X06 capacitive touch controller
//! on the correct I2C bus and interrupt pin for this board.
//!
//! # Usage
//!
//! ```no_run
//! let mut rcc = dp.RCC.freeze(...);
//! let gpiob = dp.GPIOB.split(&mut rcc);
//! let i2c = touch::init_i2c(dp.I2C1, gpiob.pb8, gpiob.pb9, &mut rcc);
//! let mut touch = touch::init_ft6x06(i2c);
//! ```

use crate::hal::gpio::alt::i2c1;
use crate::hal::i2c::I2c;
use crate::hal::pac::I2C1;
use crate::hal::prelude::*;
use crate::hal::rcc::Rcc;
use ft6x06_rs::FT6x06;

pub const FT6X06_I2C_ADDR: u8 = 0x38;

pub fn init_i2c(
    i2c: I2C1,
    pb8: impl Into<i2c1::Scl>,
    pb9: impl Into<i2c1::Sda>,
    rcc: &mut Rcc,
) -> I2c<I2C1> {
    I2c::new(i2c, (pb8, pb9), 400.kHz(), rcc)
}

pub fn init_ft6x06(i2c: I2c<I2C1>) -> FT6x06<I2c<I2C1>> {
    FT6x06::new_with_addr(i2c, FT6X06_I2C_ADDR)
}
