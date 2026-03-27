stm32f469i-disc
================
Board support package for the STM32F469I-DISCOVERY kit.
Quick Start
-----------

```toml
[dependencies.stm32f469i-disc]
git = "https://github.com/Amperstrand/stm32f469i-disc"
features = ["defmt"]
```

The default build target is `thumbv7em-none-eabihf` (see `.cargo/config.toml`).

```bash
cargo build --example gpio_hal_blinky
cargo run --example gpio_hal_blinky   # if probe-rs and board are available locally
```


Module Overview
---------------
- `lcd` - Display with auto-detection
- `led` - On-board LEDs
- `sdram` - 16MB SDRAM
- `touch` - FT6X06 controller
- `sdio` - SD card
- `button` - User button
- `usb` - USB OTG FS

Documentation Links
-------------------
- [USB Guide](docs/USB-GUIDE.md) - USB OTG FS setup and CDC-ACM
- [Pin Consumption](docs/PIN-CONSUMPTION.md) - Which pins SDRAM consumes
- [SDIO clock speeds](docs/SDIO-CLOCK-SPEEDS.md) - Specs, tradeoffs, and test results table
- [Hardware test plan](docs/HARDWARE-TEST-PLAN.md) - Run all examples on the board and record results
- [Testing Guide](../STM32F469_HAL_BSP_TESTING.md) - Full HAL/BSP testing instructions

Peripheral Support
------------------
- [x] Green, Orange, Red, Blue user LEDs
- [x] 16MB SDRAM on FMC interface
- [x] NT35510/OTM8009A LCD with DSI interface (auto-detected)
- [x] FT6X06 touch controller (I2C)
- [ ] Other on-board peripherals

Examples
--------
- `gpio_hal_blinky` — Cycle through user LEDs
- `fmc_sdram_test` — Read/write SDRAM test with pattern verification
- `display_dsi_lcd` — Rolling gradient animation on DSI display
- `display_hello_eg` — Text and shapes using embedded-graphics
- `display_touch` — Touch input with swipe gesture detection
- `usb_cdc_serial` — USB CDC-ACM virtual serial port echo test
- `sdio_raw_test` — SD card init and raw block read test (10 MiB at 1 MHz)
- `sdio_speed_sweep` — **Optional** SD clock speed test (1/4/8/12/24 MHz); requires `--features sdio-speed-test`. Run sparingly; see [SDIO clock speeds](docs/SDIO-CLOCK-SPEEDS.md) for tradeoffs and result recording.

Building
--------
Build all examples:

```bash
./scripts/build-examples.sh
```

Or build individually:

```bash
cargo build --example gpio_hal_blinky
cargo build --example fmc_sdram_test
cargo build --example display_dsi_lcd
cargo build --example display_hello_eg --features framebuffer
cargo build --example display_touch
cargo build --example usb_cdc_serial --features usb_fs
cargo build --example sdio_raw_test
cargo build --example sdio_speed_sweep --features sdio-speed-test  # optional; run sparingly
```

Binaries are under `target/thumbv7em-none-eabihf/debug/examples/<name>`.

Running on device (remote)
--------------------------
To run on the board from a host that has the probe (e.g. Ubuntu with probe-rs):

1. Copy the built ELF to the host and run with probe-rs:

```bash
# From this repo (after building)
scp target/thumbv7em-none-eabihf/debug/examples/gpio_hal_blinky ubuntu@192.168.13.246:/tmp/

# On ubuntu@192.168.13.246
/home/ubuntu/.local/bin/probe-rs run --chip STM32F469NIHx /tmp/gpio_hal_blinky
```

2. Or use the deploy-and-run script (builds, scps, and runs in one go):

```bash
./scripts/deploy-and-run.sh gpio_hal_blinky
# Other examples: fmc_sdram_test, display_dsi_lcd, display_hello_eg, display_touch, usb_cdc_serial, sdio_raw_test, sdio_speed_sweep
```

Running locally
---------------
If the board is connected to this machine and probe-rs is installed:

```bash
cargo run --example gpio_hal_blinky
```

Testing
-------
Requires an ST-Link probe and `probe-rs` installed.

Run the full fast test suite (~60s, flashes and runs all tests automatically):

    ./run_tests.sh

Run individual tests:

    ./run_tests.sh test_led         # LED on/off, toggle, patterns (16 tests)
    ./run_tests.sh test_sdram       # Fast SDRAM spot-checks (14 tests, ~10s)
    ./run_tests.sh test_sdram_full  # Exhaustive SDRAM tests, all 16MB (16 tests, ~3-5min)
    ./run_tests.sh test_gpio        # PA0 button input, GPIO output echo (5 tests)
    ./run_tests.sh test_uart        # USART1 TX, formatted output (4 tests)
    ./run_tests.sh test_timers      # TIM2/TIM3 delays, PWM, cancel (6 tests)
    ./run_tests.sh test_dma         # DMA2 memory-to-memory transfers (4 tests)
    ./run_tests.sh test_lcd         # DSI LCD init, color fills, gradient (13 tests)
    ./run_tests.sh test_usb         # USB CDC init, echo (needs host, 3 tests)
    ./run_tests.sh test_all         # All non-USB tests in one flash (~30 tests, ~60s)

Results are saved to `test-results/`.

The fast SDRAM test (`test_sdram`) thoroughly tests the first 256KB then spot-checks
16 evenly-spaced regions, scattered random probes, and the last 64KB across all 16MB.
The full variant (`test_sdram_full`) runs walking 1s/0s, checkerboard, address patterns,
March C-, and multi-pass random over the entire 16MB.

The `test_all` binary runs LED, GPIO, UART, Timer, DMA, SDRAM, and LCD tests in a
single flash, using `Peripherals::steal()` to re-acquire hardware between suites.

Credits
-------
Thanks to the authors of [stm32f429i-disc](https://github.com/stm32-rs/stm32f429i-disc.git) and [stm32f407g-disc](https://github.com/stm32-rs/stm32f407g-disc.git) crates for solid starting points.

License
-------

[0-clause BSD license](LICENSE-0BSD.txt).
