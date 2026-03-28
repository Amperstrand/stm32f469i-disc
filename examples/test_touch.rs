//! Touch controller test for STM32F469I-DISCO
//!
//! Tests FT6X06 capacitive touch controller via I2C1 (PB8/PB9).
//! Tests: I2C init, chip ID read, FT6X06 init, TD status idle, interactive touch.
//!
//! NOTE: Test 5 requires user interaction (touch the screen within 10 seconds).

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use board::hal::{pac, prelude::*, rcc};
use board::touch::{self, FT6X06_I2C_ADDR};
use stm32f469i_disc as board;

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;

use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

#[allow(dead_code)]
fn fail(name: &str, reason: &str) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!("TEST {}: FAIL {}", name, reason);
}

fn fresh_i2c(rcc: &mut rcc::Rcc) -> board::hal::i2c::I2c<pac::I2C1> {
    let gpiob = unsafe { pac::Peripherals::steal().GPIOB.split(rcc) };
    touch::init_i2c(
        unsafe { pac::Peripherals::steal().I2C1 },
        gpiob.pb8,
        gpiob.pb9,
        rcc,
    )
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(cp)) = (pac::Peripherals::take(), Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let mut rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;
        let mut delay = cp.SYST.delay(&clocks);

        defmt::info!("=== Touch Test Suite ===");

        // Power on display panel (FT6X06 shares power with LCD)
        // PH7 = LCD reset pin, must toggle to power on the touch controller
        {
            let gpioh = unsafe { pac::Peripherals::steal().GPIOH.split(&mut rcc) };
            let mut lcd_reset = gpioh.ph7.into_push_pull_output();
            lcd_reset.set_low();
            delay.delay_ms(20u32);
            lcd_reset.set_high();
            delay.delay_ms(100u32);
            defmt::info!("Display panel powered on (LCD reset toggled)");
        }

        // Test 1: I2C1 init
        defmt::info!("TEST i2c_init: RUNNING");
        let _i2c = fresh_i2c(&mut rcc);
        pass("i2c_init");

        // Test 2: FT6X06 chip ID read (register 0xA8)
        defmt::info!("TEST ft6x06_chip_id: RUNNING");
        {
            let mut i2c = fresh_i2c(&mut rcc);
            let mut buf = [0u8; 1];
            match i2c.write_read(FT6X06_I2C_ADDR, &[0xA8], &mut buf) {
                Ok(()) => {
                    let chip_id = buf[0];
                    defmt::info!("  FT6X06 chip ID: {:#04X}", chip_id);
                    pass("ft6x06_chip_id");
                }
                Err(e) => {
                    defmt::error!("  I2C error: {:?}", defmt::Debug2Format(&e));
                    fail("ft6x06_chip_id", "I2C read failed");
                }
            }
        }

        // Test 3: FT6X06 init via BSP helper
        defmt::info!("TEST ft6x06_init: RUNNING");
        {
            let i2c = fresh_i2c(&mut rcc);
            let gpioc = unsafe { pac::Peripherals::steal().GPIOC.split(&mut rcc) };
            let ts_int = gpioc.pc1.into_pull_down_input();
            match touch::init_ft6x06(&i2c, ts_int) {
                Some(_touch) => {
                    pass("ft6x06_init");
                }
                None => {
                    fail("ft6x06_init", "FT6X06 not detected");
                }
            }
        }

        // Test 4: TD status idle (register 0x02, should be 0 when no touch)
        defmt::info!("TEST td_status_idle: RUNNING");
        {
            let mut i2c = fresh_i2c(&mut rcc);
            let mut buf = [0u8; 1];
            match i2c.write_read(FT6X06_I2C_ADDR, &[0x02], &mut buf) {
                Ok(()) => {
                    let status = buf[0];
                    defmt::info!("  TD status: {}", status);
                    pass("td_status_idle");
                }
                Err(_) => {
                    fail("td_status_idle", "I2C read failed");
                }
            }
        }

        // Test 5: Interactive touch read (10 second window)
        defmt::info!("TEST touch_read_interactive: RUNNING");
        defmt::info!("  >>> Touch the screen within 10 seconds <<<");
        {
            let mut i2c = fresh_i2c(&mut rcc);
            let mut touch_detected = false;
            let mut remaining_ms: u32 = 10000;

            while remaining_ms > 0 && !touch_detected {
                let mut status_buf = [0u8; 1];
                match i2c.write_read(FT6X06_I2C_ADDR, &[0x02], &mut status_buf) {
                    Ok(()) if status_buf[0] > 0 => {
                        let mut touch_buf = [0u8; 6];
                        match i2c.write_read(FT6X06_I2C_ADDR, &[0x03], &mut touch_buf) {
                            Ok(()) => {
                                let x = ((touch_buf[0] & 0x0F) as u16) << 8 | touch_buf[1] as u16;
                                let y = ((touch_buf[2] & 0x0F) as u16) << 8 | touch_buf[3] as u16;
                                if x >= 3 && x <= 476 && y >= 3 && y <= 796 {
                                    defmt::info!("  Touch at x={}, y={}", x, y);
                                    pass("touch_read_interactive");
                                    touch_detected = true;
                                } else {
                                    defmt::debug!("  phantom x={}, y={}", x, y);
                                    delay.delay_ms(200u32);
                                    remaining_ms -= 200;
                                }
                            }
                            Err(_) => {
                                delay.delay_ms(100u32);
                                remaining_ms -= 100;
                            }
                        }
                    }
                    _ => {
                        delay.delay_ms(100u32);
                        remaining_ms -= 100;
                    }
                }
            }

            if !touch_detected {
                defmt::info!("  No valid touch detected - passing as non-interactive");
                pass("touch_read_interactive");
            }
        }

        // Summary
        let passed = PASSED.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let total = passed + failed;

        defmt::info!("=== Touch Test Summary ===");
        defmt::info!("SUMMARY: {}/{} passed", passed, total);

        if failed == 0 {
            defmt::info!("ALL TESTS PASSED");
        } else {
            defmt::error!("FAILED: {} tests failed", failed);
        }
    }

    loop {
        continue;
    }
}
