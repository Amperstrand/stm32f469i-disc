//! All-in-one hardware test for STM32F469I-DISCO
//!
//! Runs: LED + GPIO + UART + Timers + DMA + SDRAM fast + LCD
//! Excludes: USB (needs host interaction), SDRAM full (too slow)
//! Excludes: GPIO button press test (needs user interaction)
//!
//! Uses Peripherals::steal() + fresh split() for each test suite.

#![no_main]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt as rt;

use stm32f469i_disc as board;

use core::ptr::{addr_of, addr_of_mut};
use core::slice;

use cortex_m::peripheral::{Peripherals, DWT};
use cortex_m_rt::{entry, exception};

use defmt_rtt as _;
use panic_probe as _;

use board::hal::{
    dma::{config, traits::Direction, MemoryToMemory, StreamsTuple, Transfer},
    gpio::alt::fmc as alt,
    pac,
    prelude::*,
    rcc,
};
use board::led::{LedColor, Leds};
use board::sdram::{sdram_pins, Sdram};

use stm32f4xx_hal::dsi::{
    ColorCoding, DsiChannel, DsiCmdModeTransmissionKind, DsiConfig, DsiHost, DsiInterrupts,
    DsiMode, DsiPhyTimers, DsiPllConfig, DsiVideoMode, LaneCount,
};
use stm32f4xx_hal::ltdc::{DisplayConfig, DisplayController, Layer, PixelFormat};

use otm8009a::{Otm8009A, Otm8009AConfig};

use core::fmt::Write;
use core::sync::atomic::{AtomicUsize, Ordering};

use board::hal::timer::SysDelay;

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

const WIDTH: usize = 480;
const HEIGHT: usize = 800;

pub const DISPLAY_CFG: DisplayConfig = DisplayConfig {
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

fn print_summary() {
    let passed = PASSED.load(Ordering::Relaxed);
    let failed = FAILED.load(Ordering::Relaxed);
    let total = passed + failed;
    defmt::info!("=== ALL TESTS SUMMARY ===");
    defmt::info!("SUMMARY: {}/{} passed", passed, total);
    if failed == 0 {
        defmt::info!("ALL TESTS PASSED");
    } else {
        defmt::error!("FAILED: {} tests failed", failed);
    }
}

unsafe fn fresh_rcc() -> rcc::Rcc {
    unsafe {
        pac::Peripherals::steal()
            .RCC
            .constrain()
            .freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()))
    }
}

// ===== DMA helpers =====

static mut DMA_SRC1: [u8; 64] = [0; 64];
static mut DMA_DST1: [u8; 64] = [0; 64];
static mut DMA_SRC2: [u8; 4096] = [0; 4096];
static mut DMA_DST2: [u8; 4096] = [0; 4096];

unsafe fn dma_fill(buf: *mut u8, len: usize, pat: u8) {
    for i in 0..len {
        unsafe {
            *buf.add(i) = (i as u8).wrapping_mul(pat);
        }
    }
}

unsafe fn dma_verify(src: *const u8, dst: *const u8, len: usize) -> bool {
    for i in 0..len {
        unsafe {
            if *src.add(i) != *dst.add(i) {
                return false;
            }
        }
    }
    true
}

unsafe fn dma2_xfer(dst: *mut u8, src: *const u8, len: usize) {
    unsafe {
        let dp = pac::Peripherals::steal();
        let mut rcc = pac::Peripherals::steal()
            .RCC
            .constrain()
            .freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
        let streams = StreamsTuple::new(dp.DMA2, &mut rcc);
        let stream = streams.0;
        let cfg = config::DmaConfig::default()
            .memory_increment(true)
            .peripheral_increment(true)
            .fifo_enable(true);
        let dst_buf: &'static mut [u8] = &mut *core::ptr::slice_from_raw_parts_mut(dst, len);
        let src_buf: &'static mut [u8] =
            &mut *core::ptr::slice_from_raw_parts_mut(src as *mut u8, len);
        let mut t: Transfer<_, 0, MemoryToMemory<u8>, MemoryToMemory<u8>, &'static mut [u8]> =
            Transfer::init_memory_to_memory(
                stream,
                MemoryToMemory::<u8>::new(),
                dst_buf,
                src_buf,
                cfg,
            );
        t.start(|_| {});
        t.wait();
    }
}

// ===== SDRAM test helpers =====

struct XorShift32 {
    seed: u32,
}
impl XorShift32 {
    fn new(s: u32) -> Self {
        XorShift32 {
            seed: if s == 0 { 1 } else { s },
        }
    }
    fn next(&mut self) -> u32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed
    }
}

// ===== Test suites =====

fn test_led(delay: &mut SysDelay) {
    defmt::info!("--- LED Tests ---");
    unsafe {
        let mut rcc = fresh_rcc();
        let gpiod = pac::Peripherals::steal().GPIOD.split(&mut rcc);
        let gpiog = pac::Peripherals::steal().GPIOG.split(&mut rcc);
        let gpiok = pac::Peripherals::steal().GPIOK.split(&mut rcc);
        let mut leds = Leds::new(gpiod, gpiog, gpiok);

        // Individual LED tests
        let names = ["led_green", "led_orange", "led_red", "led_blue"];
        for (i, name) in names.iter().enumerate() {
            defmt::info!("TEST {}: RUNNING", name);
            leds[i].on();
            delay.delay_ms(50u32);
            leds[i].off();
            delay.delay_ms(50u32);
            pass(name);
        }

        // All on/off
        defmt::info!("TEST led_all_on: RUNNING");
        for led in leds.iter_mut() {
            led.on();
        }
        delay.delay_ms(300u32);
        pass("led_all_on");

        defmt::info!("TEST led_all_off: RUNNING");
        for led in leds.iter_mut() {
            led.off();
        }
        delay.delay_ms(100u32);
        pass("led_all_off");

        // Rapid toggle
        defmt::info!("TEST led_rapid: RUNNING");
        for _ in 0..50 {
            for led in leds.iter_mut() {
                led.on();
            }
            for led in leds.iter_mut() {
                led.off();
            }
        }
        pass("led_rapid");

        // Index by color
        defmt::info!("TEST led_by_color: RUNNING");
        for c in [
            LedColor::Green,
            LedColor::Orange,
            LedColor::Red,
            LedColor::Blue,
        ] {
            leds[c].on();
            delay.delay_ms(30u32);
            leds[c].off();
        }
        pass("led_by_color");
    }
}

fn test_gpio(delay: &mut SysDelay) {
    defmt::info!("--- GPIO Tests ---");
    unsafe {
        let mut rcc = fresh_rcc();
        let gpioa = pac::Peripherals::steal().GPIOA.split(&mut rcc);

        // Input mode test
        defmt::info!("TEST pa0_input: RUNNING");
        let _button = gpioa.pa0.into_pull_down_input();
        pass("pa0_input");

        // Multi-port output
        defmt::info!("TEST multi_port_out: RUNNING");
        let gpiod = pac::Peripherals::steal().GPIOD.split(&mut rcc);
        let gpiog = pac::Peripherals::steal().GPIOG.split(&mut rcc);
        let gpiok = pac::Peripherals::steal().GPIOK.split(&mut rcc);
        let mut g = gpiog.pg6.into_push_pull_output();
        let mut o = gpiod.pd4.into_push_pull_output();
        let mut r = gpiod.pd5.into_push_pull_output();
        let mut b = gpiok.pk3.into_push_pull_output();
        g.set_high();
        o.set_high();
        r.set_high();
        b.set_high();
        delay.delay_ms(200u32);
        g.set_low();
        o.set_low();
        r.set_low();
        b.set_low();
        delay.delay_ms(100u32);
        pass("multi_port_out");
    }
}

fn test_uart(delay: &mut SysDelay) {
    defmt::info!("--- UART Tests ---");
    unsafe {
        let mut rcc = fresh_rcc();
        let gpioa = pac::Peripherals::steal().GPIOA.split(&mut rcc);

        defmt::info!("TEST usart1_init: RUNNING");
        let mut tx: board::hal::serial::Tx<pac::USART1> =
            match pac::Peripherals::steal()
                .USART1
                .tx(gpioa.pa9, 115200.bps(), &mut rcc)
            {
                Ok(tx) => {
                    pass("usart1_init");
                    tx
                }
                Err(_) => {
                    fail("usart1_init", "init failed");
                    return;
                }
            };

        defmt::info!("TEST usart1_tx_byte: RUNNING");
        if tx.write(b'U').is_ok() {
            pass("usart1_tx_byte");
        } else {
            fail("usart1_tx_byte", "write error");
        }

        defmt::info!("TEST usart1_fmt: RUNNING");
        if write!(tx, "test_all OK\r\n").is_ok() {
            pass("usart1_fmt");
        } else {
            fail("usart1_fmt", "fmt error");
        }

        defmt::info!("TEST usart1_multi: RUNNING");
        let mut ok = true;
        for b in b"HELLO" {
            if tx.write(*b).is_err() {
                ok = false;
                break;
            }
        }
        if ok {
            pass("usart1_multi");
        } else {
            fail("usart1_multi", "write failed");
        }
    }
    delay.delay_ms(10u32);
}

fn test_timers() {
    defmt::info!("--- Timer Tests ---");
    unsafe {
        let mut rcc = fresh_rcc();

        defmt::info!("TEST tim2_1ms: RUNNING");
        {
            let mut ctr = pac::Peripherals::steal().TIM2.counter_us(&mut rcc);
            let start = DWT::cycle_count();
            ctr.start(1.millis()).unwrap();
            let _ = ctr.wait();
            let us = DWT::cycle_count().wrapping_sub(start) / 180;
            if us >= 900 && us <= 1500 {
                pass("tim2_1ms");
            } else {
                fail("tim2_1ms", "out of range");
            }
        }

        defmt::info!("TEST tim3_100ms: RUNNING");
        {
            let mut ctr = pac::Peripherals::steal().TIM3.counter_ms(&mut rcc);
            let start = DWT::cycle_count();
            ctr.start(100.millis()).unwrap();
            let _ = ctr.wait();
            let ms = DWT::cycle_count().wrapping_sub(start) / 180_000;
            if ms >= 95 && ms <= 120 {
                pass("tim3_100ms");
            } else {
                fail("tim3_100ms", "out of range");
            }
        }

        defmt::info!("TEST tim3_pwm: RUNNING");
        {
            let gpioa = pac::Peripherals::steal().GPIOA.split(&mut rcc);
            let (_pwm, (ch1, _ch2, _ch3, _ch4)) =
                pac::Peripherals::steal().TIM3.pwm_hz(10.kHz(), &mut rcc);
            let mut ch1 = ch1.with(gpioa.pa6);
            let mx = ch1.get_duty();
            ch1.set_duty(mx / 2);
            ch1.enable();
            ch1.set_duty(mx / 4);
            ch1.set_duty(0);
            ch1.set_duty(mx);
            ch1.disable();
            pass("tim3_pwm");
        }

        defmt::info!("TEST tim2_cancel: RUNNING");
        {
            let mut ctr = pac::Peripherals::steal().TIM2.counter_us(&mut rcc);
            ctr.start(10.secs()).unwrap();
            let _ = ctr.cancel();
            let start = DWT::cycle_count();
            let _ = ctr.cancel();
            let elapsed = DWT::cycle_count().wrapping_sub(start);
            if elapsed < 180_000 {
                pass("tim2_cancel");
            } else {
                fail("tim2_cancel", "cancel took too long");
            }
        }
    }
}

fn test_dma() {
    defmt::info!("--- DMA Tests ---");
    unsafe {
        defmt::info!("TEST dma_64b: RUNNING");
        dma_fill(addr_of_mut!(DMA_SRC1) as *mut u8, 64, 0xAB);
        dma_fill(addr_of_mut!(DMA_DST1) as *mut u8, 64, 0);
        dma2_xfer(
            addr_of_mut!(DMA_DST1) as *mut u8,
            addr_of!(DMA_SRC1) as *const u8,
            64,
        );
        if dma_verify(
            addr_of!(DMA_SRC1) as *const u8,
            addr_of!(DMA_DST1) as *const u8,
            64,
        ) {
            pass("dma_64b");
        } else {
            fail("dma_64b", "mismatch");
        }

        defmt::info!("TEST dma_4096b: RUNNING");
        dma_fill(addr_of_mut!(DMA_SRC2) as *mut u8, 4096, 1);
        dma_fill(addr_of_mut!(DMA_DST2) as *mut u8, 4096, 0);
        dma2_xfer(
            addr_of_mut!(DMA_DST2) as *mut u8,
            addr_of!(DMA_SRC2) as *const u8,
            4096,
        );
        if dma_verify(
            addr_of!(DMA_SRC2) as *const u8,
            addr_of!(DMA_DST2) as *const u8,
            4096,
        ) {
            pass("dma_4096b");
        } else {
            fail("dma_4096b", "mismatch");
        }

        defmt::info!("TEST dma_repeated: RUNNING");
        let mut ok = true;
        for r in 0..10u32 {
            dma_fill(addr_of_mut!(DMA_SRC1) as *mut u8, 64, (r & 0xFF) as u8);
            dma_fill(addr_of_mut!(DMA_DST1) as *mut u8, 64, 0);
            dma2_xfer(
                addr_of_mut!(DMA_DST1) as *mut u8,
                addr_of!(DMA_SRC1) as *const u8,
                64,
            );
            if !dma_verify(
                addr_of!(DMA_SRC1) as *const u8,
                addr_of!(DMA_DST1) as *const u8,
                64,
            ) {
                ok = false;
                break;
            }
        }
        if ok {
            pass("dma_repeated");
        } else {
            fail("dma_repeated", "mismatch");
        }
    }
}

fn test_sdram(delay: &mut SysDelay) {
    defmt::info!("--- SDRAM Fast Tests ---");
    unsafe {
        let mut rcc = fresh_rcc();
        let clocks = rcc.clocks;
        let gpioc = pac::Peripherals::steal().GPIOC.split(&mut rcc);
        let gpiod = pac::Peripherals::steal().GPIOD.split(&mut rcc);
        let gpioe = pac::Peripherals::steal().GPIOE.split(&mut rcc);
        let gpiof = pac::Peripherals::steal().GPIOF.split(&mut rcc);
        let gpiog = pac::Peripherals::steal().GPIOG.split(&mut rcc);
        let gpioh = pac::Peripherals::steal().GPIOH.split(&mut rcc);
        let gpioi = pac::Peripherals::steal().GPIOI.split(&mut rcc);

        defmt::info!("TEST sdram_init: RUNNING");
        let sdram = Sdram::new(
            pac::Peripherals::steal().FMC,
            sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
            &clocks,
            delay,
        );
        pass("sdram_init");

        let words = sdram.words;
        let base = sdram.mem as usize;
        let ram: &mut [u32] = slice::from_raw_parts_mut(sdram.mem, words);

        // Checkerboard
        defmt::info!("TEST sdram_checkerboard: RUNNING");
        {
            let mut ok = true;
            let win = core::cmp::min(65536, words);
            for w in ram[..win].iter_mut() {
                *w = 0xAAAAAAAA;
            }
            for (_i, w) in ram[..win].iter().enumerate() {
                if *w != 0xAAAAAAAA {
                    fail("sdram_checkerboard", "mismatch");
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("sdram_checkerboard");
            }
        }

        // Inverse checkerboard
        defmt::info!("TEST sdram_inv_check: RUNNING");
        {
            let mut ok = true;
            let win = core::cmp::min(65536, words);
            for w in ram[..win].iter_mut() {
                *w = 0x55555555;
            }
            for (_i, w) in ram[..win].iter().enumerate() {
                if *w != 0x55555555 {
                    fail("sdram_inv_check", "mismatch");
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("sdram_inv_check");
            }
        }

        // Address pattern
        defmt::info!("TEST sdram_addr: RUNNING");
        {
            let mut ok = true;
            let win = core::cmp::min(65536, words);
            for (i, w) in ram[..win].iter_mut().enumerate() {
                *w = (base + i * 4) as u32;
            }
            for (i, w) in ram[..win].iter().enumerate() {
                if *w != (base + i * 4) as u32 {
                    fail("sdram_addr", "mismatch");
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("sdram_addr");
            }
        }

        // Random
        defmt::info!("TEST sdram_random: RUNNING");
        {
            let mut ok = true;
            let win = core::cmp::min(65536, words);
            let mut rng = XorShift32::new(0xDEADBEEF);
            for w in ram[..win].iter_mut() {
                *w = rng.next();
            }
            let mut rng = XorShift32::new(0xDEADBEEF);
            for (_i, w) in ram[..win].iter().enumerate() {
                let exp = rng.next();
                if *w != exp {
                    fail("sdram_random", "mismatch");
                    ok = false;
                    break;
                }
            }
            if ok {
                pass("sdram_random");
            }
        }

        // Boundary spots
        defmt::info!("TEST sdram_boundary: RUNNING");
        {
            let mut ok = true;
            let region_size = 1024;
            let num_regions = 16;
            let stride = words / num_regions;
            for r in 0..num_regions {
                let offset = r * stride;
                let pattern = 0xFEED0000 | (r as u32);
                let end = core::cmp::min(offset + region_size, words);
                for w in ram[offset..end].iter_mut() {
                    *w = pattern;
                }
            }
            for r in 0..num_regions {
                let offset = r * stride;
                let pattern = 0xFEED0000 | (r as u32);
                let end = core::cmp::min(offset + region_size, words);
                for (_i, w) in ram[offset..end].iter().enumerate() {
                    if *w != pattern {
                        fail("sdram_boundary", "mismatch");
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if ok {
                pass("sdram_boundary");
            }
        }

        let _ = (base, words);
    }
}

fn test_lcd(delay: &mut SysDelay) {
    defmt::info!("--- LCD Tests ---");
    unsafe {
        let hse_freq = 8.MHz();
        let mut rcc = pac::Peripherals::steal()
            .RCC
            .constrain()
            .freeze(rcc::Config::hse(hse_freq).pclk2(32.MHz()).sysclk(180.MHz()));
        let clocks = rcc.clocks;

        let _gpioa = pac::Peripherals::steal().GPIOA.split(&mut rcc);
        let gpioc = pac::Peripherals::steal().GPIOC.split(&mut rcc);
        let gpiod = pac::Peripherals::steal().GPIOD.split(&mut rcc);
        let gpioe = pac::Peripherals::steal().GPIOE.split(&mut rcc);
        let gpiof = pac::Peripherals::steal().GPIOF.split(&mut rcc);
        let gpiog = pac::Peripherals::steal().GPIOG.split(&mut rcc);
        let gpioh = pac::Peripherals::steal().GPIOH.split(&mut rcc);
        let gpioi = pac::Peripherals::steal().GPIOI.split(&mut rcc);

        // SDRAM for framebuffer
        defmt::info!("TEST lcd_sdram: RUNNING");
        let sdram = Sdram::new(
            pac::Peripherals::steal().FMC,
            sdram_pins! {gpioc, gpiod, gpioe, gpiof, gpiog, gpioh, gpioi},
            &clocks,
            delay,
        );
        pass("lcd_sdram");

        let framebuffer = slice::from_raw_parts_mut(sdram.mem, WIDTH * HEIGHT);

        // LCD reset
        defmt::info!("TEST lcd_reset: RUNNING");
        {
            let gpioh = pac::Peripherals::steal().GPIOH.split(&mut rcc);
            let mut lcd_reset = gpioh.ph7.into_push_pull_output();
            lcd_reset.set_low();
            delay.delay_ms(20u32);
            lcd_reset.set_high();
            delay.delay_ms(10u32);
        }
        pass("lcd_reset");

        // LTDC
        defmt::info!("TEST lcd_ltdc: RUNNING");
        let ltdc_freq = 27_429.kHz();
        let mut display = DisplayController::<u32>::new(
            pac::Peripherals::steal().LTDC,
            pac::Peripherals::steal().DMA2D,
            None,
            PixelFormat::ARGB8888,
            DISPLAY_CFG,
            Some(hse_freq),
        );
        display.config_layer(Layer::L1, framebuffer, PixelFormat::ARGB8888);
        display.enable_layer(Layer::L1);
        display.reload();
        pass("lcd_ltdc");

        // DSI
        defmt::info!("TEST lcd_dsi: RUNNING");
        let dsi_pll = DsiPllConfig::manual(125, 2, 0, 4);
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
        let mut dsi_host = match DsiHost::init(
            dsi_pll,
            DISPLAY_CFG,
            dsi_cfg,
            pac::Peripherals::steal().DSI,
            &mut rcc,
        ) {
            Ok(h) => {
                pass("lcd_dsi");
                h
            }
            Err(_) => {
                fail("lcd_dsi", "init failed");
                return;
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
        defmt::info!("TEST lcd_otm: RUNNING");
        let otm_cfg = Otm8009AConfig {
            frame_rate: otm8009a::FrameRate::_60Hz,
            mode: otm8009a::Mode::Portrait,
            color_map: otm8009a::ColorMap::Rgb,
            cols: WIDTH as u16,
            rows: HEIGHT as u16,
        };
        let mut otm = Otm8009A::new();
        match otm.init(&mut dsi_host, otm_cfg, delay) {
            Ok(_) => {
                pass("lcd_otm");
            }
            Err(_) => {
                fail("lcd_otm", "init failed");
                return;
            }
        }
        let _ = otm.enable_te_output(533, &mut dsi_host);
        dsi_host.set_command_mode_transmission_kind(DsiCmdModeTransmissionKind::AllInHighSpeed);
        dsi_host.force_rx_low_power(true);
        dsi_host.refresh();

        let fb = slice::from_raw_parts_mut(sdram.mem, sdram.words);

        // Color fills
        for (name, color) in [
            ("lcd_red", 0x00FF0000u32),
            ("lcd_green", 0x0000FF00),
            ("lcd_blue", 0x000000FF),
            ("lcd_white", 0x00FFFFFF),
        ] {
            defmt::info!("TEST {}: RUNNING", name);
            for px in fb.iter_mut().take(WIDTH * HEIGHT) {
                *px = color;
            }
            delay.delay_ms(200u32);
            pass(name);
        }

        // Clear to black
        for px in fb.iter_mut().take(WIDTH * HEIGHT) {
            *px = 0;
        }
    }
}

#[entry]
fn main() -> ! {
    let dp = pac::Peripherals::take().unwrap();
    let mut cp = Peripherals::take().unwrap();

    let rcc = dp.RCC.constrain();
    let rcc = rcc.freeze(rcc::Config::hse(8.MHz()).sysclk(180.MHz()));
    let clocks = rcc.clocks;
    let mut delay = cp.SYST.delay(&clocks);

    cp.SCB.invalidate_icache();
    cp.SCB.enable_icache();
    cp.DWT.enable_cycle_counter();

    defmt::info!("=== All-In-One Test Suite ===");

    test_led(&mut delay);
    test_gpio(&mut delay);
    test_uart(&mut delay);
    test_timers();
    test_dma();
    test_sdram(&mut delay);
    test_lcd(&mut delay);

    print_summary();

    loop {
        continue;
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
