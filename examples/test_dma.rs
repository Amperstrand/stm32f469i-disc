//! DMA test for STM32F469I-DISCO
//!
//! Tests DMA2 stream0 memory-to-memory transfers (byte-level).
//! Uses raw pointers to satisfy Rust 2024 static mut reference rules.

#![no_main]
#![no_std]

use defmt_rtt as _;
use panic_probe as _;

use stm32f469i_disc as board;

use board::hal::{
    dma::{config, traits::Direction, MemoryToMemory, StreamsTuple, Transfer},
    pac,
    prelude::*,
    rcc::Config,
};

use cortex_m_rt::entry;

use core::ptr::{addr_of, addr_of_mut};
use core::sync::atomic::{AtomicUsize, Ordering};

static PASSED: AtomicUsize = AtomicUsize::new(0);
static FAILED: AtomicUsize = AtomicUsize::new(0);

fn pass(name: &str) {
    PASSED.fetch_add(1, Ordering::Relaxed);
    defmt::info!("TEST {}: PASS", name);
}

fn fail(name: &str, reason: &str) {
    FAILED.fetch_add(1, Ordering::Relaxed);
    defmt::error!("TEST {}: FAIL {}", name, reason);
}

static mut SRC1: [u8; 64] = [0; 64];
static mut DST1: [u8; 64] = [0; 64];
static mut SRC2: [u8; 4096] = [0; 4096];
static mut DST2: [u8; 4096] = [0; 4096];
static mut SRC3: [u8; 1024] = [0; 1024];
static mut DST3: [u8; 1024] = [0; 1024];
static mut SRC4: [u8; 256] = [0; 256];
static mut DST4: [u8; 256] = [0; 256];

unsafe fn fill_u8(buf: *mut u8, len: usize, pattern: u8) {
    for i in 0..len {
        unsafe {
            *buf.add(i) = (i as u8).wrapping_mul(pattern);
        }
    }
}

unsafe fn verify_u8(src: *const u8, dst: *const u8, len: usize) -> bool {
    for i in 0..len {
        unsafe {
            if *src.add(i) != *dst.add(i) {
                return false;
            }
        }
    }
    true
}

unsafe fn dma2_stream0_transfer(dst: *mut u8, src: *const u8, len: usize) {
    unsafe {
        let dp = pac::Peripherals::steal();
        let mut rcc = pac::Peripherals::steal()
            .RCC
            .constrain()
            .freeze(Config::hse(8.MHz()).sysclk(180.MHz()));
        let streams = StreamsTuple::new(dp.DMA2, &mut rcc);

        let stream = streams.0;

        let dma_cfg = config::DmaConfig::default()
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
                dma_cfg,
            );
        t.start(|_| {});
        t.wait();
    }
}

#[entry]
fn main() -> ! {
    if let (Some(p), Some(_cp)) = (pac::Peripherals::take(), cortex_m::Peripherals::take()) {
        let rcc = p.RCC.constrain();
        let _rcc = rcc.freeze(Config::hse(8.MHz()).sysclk(180.MHz()));

        defmt::info!("=== DMA Test Suite ===");

        // Test 1: 64-byte transfer
        defmt::info!("TEST dma_64b: RUNNING");
        unsafe {
            fill_u8(addr_of_mut!(SRC1) as *mut u8, 64, 0xAB);
            fill_u8(addr_of_mut!(DST1) as *mut u8, 64, 0);
            dma2_stream0_transfer(
                addr_of_mut!(DST1) as *mut u8,
                addr_of!(SRC1) as *const u8,
                64,
            );
            if verify_u8(addr_of!(SRC1) as *const u8, addr_of!(DST1) as *const u8, 64) {
                pass("dma_64b");
            } else {
                fail("dma_64b", "data mismatch");
            }
        }

        // Test 2: 4096-byte transfer
        defmt::info!("TEST dma_4096b: RUNNING");
        unsafe {
            fill_u8(addr_of_mut!(SRC2) as *mut u8, 4096, 1);
            fill_u8(addr_of_mut!(DST2) as *mut u8, 4096, 0);
            dma2_stream0_transfer(
                addr_of_mut!(DST2) as *mut u8,
                addr_of!(SRC2) as *const u8,
                4096,
            );
            if verify_u8(
                addr_of!(SRC2) as *const u8,
                addr_of!(DST2) as *const u8,
                4096,
            ) {
                pass("dma_4096b");
            } else {
                fail("dma_4096b", "data mismatch");
            }
        }

        // Test 3: 1024-byte transfer
        defmt::info!("TEST dma_1024b: RUNNING");
        unsafe {
            fill_u8(addr_of_mut!(SRC3) as *mut u8, 1024, 0xFF);
            fill_u8(addr_of_mut!(DST3) as *mut u8, 1024, 0);
            dma2_stream0_transfer(
                addr_of_mut!(DST3) as *mut u8,
                addr_of!(SRC3) as *const u8,
                1024,
            );
            if verify_u8(
                addr_of!(SRC3) as *const u8,
                addr_of!(DST3) as *const u8,
                1024,
            ) {
                pass("dma_1024b");
            } else {
                fail("dma_1024b", "data mismatch");
            }
        }

        // Test 4: Repeated transfers (10 rounds)
        defmt::info!("TEST dma_repeated: RUNNING");
        {
            let mut ok = true;
            for round in 0..10u32 {
                unsafe {
                    fill_u8(addr_of_mut!(SRC4) as *mut u8, 256, (round & 0xFF) as u8);
                    fill_u8(addr_of_mut!(DST4) as *mut u8, 256, 0);
                    dma2_stream0_transfer(
                        addr_of_mut!(DST4) as *mut u8,
                        addr_of!(SRC4) as *const u8,
                        256,
                    );
                    if !verify_u8(
                        addr_of!(SRC4) as *const u8,
                        addr_of!(DST4) as *const u8,
                        256,
                    ) {
                        ok = false;
                    }
                }
                if !ok {
                    fail("dma_repeated", "mismatch on round");
                    break;
                }
            }
            if ok {
                pass("dma_repeated");
            }
        }

        let passed = PASSED.load(Ordering::Relaxed);
        let failed = FAILED.load(Ordering::Relaxed);
        let total = passed + failed;

        defmt::info!("=== DMA Test Summary ===");
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
