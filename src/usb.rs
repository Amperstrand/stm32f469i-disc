//! USB OTG FS initialization for STM32F469I-DISCO board
//!
//! Provides USB peripheral setup using the OTG FS interface.
//! Uses PA11 (DM) and PA12 (DP) pins.
//!
//! # Usage
//!
//! ```no_run
//! let gpioa = dp.GPIOA.split(&mut rcc);
//! let usb = usb::init(
//!     (dp.OTG_FS_GLOBAL, dp.OTG_FS_DEVICE, dp.OTG_FS_PWRCLK),
//!     gpioa.pa11,
//!     gpioa.pa12,
//!     &rcc.clocks,
//! );
//! // Pass to SerialDriver or use with UsbBus::new(usb, ep_memory)
//! ```

use crate::hal;
use crate::hal::otg_fs::USB;

const USB_OTG_FS_GLOBAL_BASE: usize = 0x5000_0000;
const USB_OTG_FS_GRSTCTL_OFFSET: usize = 0x010;
const USB_OTG_FS_GCCFG_OFFSET: usize = 0x038;

const RCC_AHB2ENR_OTGFSEN: u32 = 1 << 7;
const RCC_AHB2RSTR_OTGFSRST: u32 = 1 << 7;
const USB_OTG_GRSTCTL_AHBIDL: u32 = 1 << 31;
const USB_OTG_GRSTCTL_CSRST: u32 = 1 << 0;
const USB_OTG_GCCFG_PWRDWN: u32 = 1 << 16;

/// Reset the USB OTG FS peripheral and PHY for clean re-enumeration.
///
/// Call this before [`USB::new`] / [`init`] when bringing up USB after an
/// `st-flash` soft reset. `SYSRESETREQ` can leave the OTG FS core and PHY in a
/// stale state where the device does not re-enumerate until the PHY is fully
/// power-cycled.
///
/// The reset sequence matches the proven Embassy BSP flow:
/// 1. Gate the OTG FS RCC clock off and back on
/// 2. Pulse the OTG FS peripheral reset in RCC
/// 3. Wait for AHB idle, then issue a core soft reset via `GRSTCTL.CSRST`
/// 4. Power-cycle the PHY via `GCCFG.PWRDWN`
pub fn reset_usb_phy() {
    let rcc = unsafe { &*hal::pac::RCC::ptr() };

    rcc.ahb2enr()
        .modify(|r, w| unsafe { w.bits(r.bits() & !RCC_AHB2ENR_OTGFSEN) });
    cortex_m::asm::delay(100);
    rcc.ahb2enr()
        .modify(|r, w| unsafe { w.bits(r.bits() | RCC_AHB2ENR_OTGFSEN) });

    rcc.ahb2rstr()
        .modify(|r, w| unsafe { w.bits(r.bits() | RCC_AHB2RSTR_OTGFSRST) });
    cortex_m::asm::delay(100);
    rcc.ahb2rstr()
        .modify(|r, w| unsafe { w.bits(r.bits() & !RCC_AHB2RSTR_OTGFSRST) });
    cortex_m::asm::delay(100);

    let grstctl = (USB_OTG_FS_GLOBAL_BASE + USB_OTG_FS_GRSTCTL_OFFSET) as *mut u32;
    let gccfg = (USB_OTG_FS_GLOBAL_BASE + USB_OTG_FS_GCCFG_OFFSET) as *mut u32;

    unsafe {
        while grstctl.read_volatile() & USB_OTG_GRSTCTL_AHBIDL == 0 {}

        grstctl.write_volatile(USB_OTG_GRSTCTL_CSRST);
        while grstctl.read_volatile() & USB_OTG_GRSTCTL_CSRST != 0 {}

        gccfg.write_volatile(0);
        cortex_m::asm::delay(100);
        gccfg.write_volatile(USB_OTG_GCCFG_PWRDWN);
    }
}

/// Initialize the USB OTG FS peripheral.
///
/// Configures PA11 (DM) and PA12 (DP) in alternate function mode
/// for USB device operation.
///
/// # Arguments
///
/// * `periphs` - Tuple of (OTG_FS_GLOBAL, OTG_FS_DEVICE, OTG_FS_PWRCLK)
/// * `pa11` - USB DM pin (PA11)
/// * `pa12` - USB DP pin (PA12)
/// * `clocks` - System clocks reference
///
/// # Returns
///
/// A `USB` struct ready for use with `UsbBus::new(usb, ep_memory)`.
pub fn init(
    periphs: (
        hal::pac::OTG_FS_GLOBAL,
        hal::pac::OTG_FS_DEVICE,
        hal::pac::OTG_FS_PWRCLK,
    ),
    pa11: hal::gpio::PA11,
    pa12: hal::gpio::PA12,
    clocks: &hal::rcc::Clocks,
) -> USB {
    USB::new(periphs, (pa11, pa12), clocks)
}
