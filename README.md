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

Board Hardware
--------------
The STM32F469I-DISCO has these on-board peripherals:

| Peripheral | Chip | Interface | BSP Status |
|---|---|---|---|
| 16MB SDRAM | IS42S32400F-6BL | FMC | Supported, tested |
| 4" TFT LCD (480x800) | NT35510/OTM8009A | MIPI DSI + LTDC | Supported, tested |
| Capacitive touch | FT6X06 | I2C1 | Supported, tested |
| USB OTG FS | Built-in | PA11/PA12 | Supported, tested |
| 4 User LEDs | - | GPIO | Supported, tested |
| User button | - | PA0 | Supported, tested |
| MicroSD slot | - | SDIO | Supported, untested |
| Hardware RNG | Built-in | RNG | Supported, tested |
| Internal temp sensor | Built-in | ADC1 | MCU internal, tested |
| SAI Audio DAC | External | SAI | Not implemented |
| 3 MEMS microphones | - | DFSDM | Not implemented |
| 16MB QSPI NOR Flash | - | QUADSPI | Not implemented |

Not on this board: no accelerometer, no gyroscope, no external temperature sensor.

Peripheral Support
------------------
- [x] Green, Orange, Red, Blue user LEDs
- [x] 16MB SDRAM on FMC interface
- [x] NT35510/OTM8009A LCD with DSI interface (auto-detected)
- [x] FT6X06 touch controller (I2C)
- [x] USB OTG FS (CDC-ACM)
- [x] Hardware RNG
- [x] Internal ADC temperature sensor
- [ ] SAI Audio DAC + headphone jack
- [ ] DFSDM MEMS microphones (x3)
- [ ] QSPI NOR Flash

Examples
--------
- `gpio_hal_blinky` - Cycle through user LEDs
- `fmc_sdram_test` - Read/write SDRAM test with pattern verification
- `display_dsi_lcd` - Rolling gradient animation on DSI display
- `display_hello_eg` - Text and shapes using embedded-graphics
- `display_touch` - Touch input with swipe gesture detection
- `usb_cdc_serial` - USB CDC-ACM virtual serial port echo test
- `sdio_raw_test` - SD card init and raw block read test (10 MiB at 1 MHz)
- `sdio_speed_sweep` - **Optional** SD clock speed test; requires `--features sdio-speed-test`

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
cargo build --example usb_cdc_serial
cargo build --example sdio_raw_test
```

Binaries are under `target/thumbv7em-none-eabihf/release/examples/<name>`.

Running on device (remote)
--------------------------
To run on the board from a host that has the probe (e.g. Ubuntu with probe-rs):

1. Copy the built ELF to the host and run with probe-rs:

```bash
scp target/thumbv7em-none-eabihf/release/examples/gpio_hal_blinky ubuntu@192.168.13.246:/tmp/

# On ubuntu@192.168.13.246
probe-rs run --chip STM32F469NIHx /tmp/gpio_hal_blinky
```

2. Or use the deploy-and-run script (builds, scps, and runs in one go):

```bash
./scripts/deploy-and-run.sh gpio_hal_blinky
```

Running locally
---------------
If the board is connected to this machine and probe-rs is installed:

```bash
cargo run --example gpio_hal_blinky
```

Testing
-------
Two testing modes are supported:

### Mode 1: Debug Probe Tests (probe-rs + RTT)
Requires an ST-Link probe and `probe-rs` installed.

Run the full test suite (~2 min, flashes and runs all tests automatically):

    ./run_tests.sh

Run individual tests:

    ./run_tests.sh test_led         # LED on/off, toggle, patterns (16 tests)
    ./run_tests.sh test_sdram       # Fast SDRAM spot-checks (14 tests, ~10s)
    ./run_tests.sh test_sdram_full  # Exhaustive SDRAM tests, all 16MB (16 tests, ~3-5min)
    ./run_tests.sh test_gpio        # PA0 button input, GPIO output (5 tests)
    ./run_tests.sh test_uart        # USART1 TX, formatted output (4 tests)
    ./run_tests.sh test_timers      # TIM2/TIM3 delays, PWM, cancel (8 tests)
    ./run_tests.sh test_dma         # DMA2 mem-to-mem transfers (4 tests)
    ./run_tests.sh test_lcd         # DSI LCD init, color fills (13 tests)
    ./run_tests.sh test_all         # All non-USB in one flash (~42 tests, ~60s)

Results are saved to `test-results/`.

### Mode 2: USB Standalone Test (no debug probe)
USB CDC timing is sensitive. When probe-rs is attached for RTT logging, it halts
the CPU periodically, causing USB disconnects. For reliable USB testing:

    ./scripts/usb_test.sh [duration_seconds]

This script:
1. Builds `test_usb_standalone` (no RTT, no defmt, no panic_probe)
2. Flashes with `st-flash --connect-under-reset` (not probe-rs)
3. Resets the device
4. Runs `tests/host/test_usb_host.py` to test USB CDC via serial

The Python host test verifies: enumeration, PING/PONG echo, multi-byte echo,
sustained stress, and graceful shutdown.

You can also run the Python test standalone:

    python3 tests/host/test_usb_host.py --device /dev/ttyACM0 --duration 60

### Test Coverage Summary

All probe-rs tests verified on STM32F469I-DISCO B08 (NT35510 panel) on 2026-03-28.

| Test | Peripherals | Tests | Device | Mode |
|---|---|---|---|---|
| `test_led` | GPIO | 16 | PASS | Probe |
| `test_sdram` | FMC | 14 | PASS | Probe |
| `test_sdram_full` | FMC | 13 | PASS | Probe |
| `test_gpio` | GPIO, Button | 5 | PASS | Probe |
| `test_uart` | USART1 | 4 | PASS | Probe |
| `test_timers` | TIM2, TIM3, DWT | 8 | PASS | Probe |
| `test_dma` | DMA2 | 4 | PASS | Probe |
| `test_lcd` | DSI, LTDC, OTM8009A | 13 | PASS | Probe |
| `test_all` | All above + RNG + ADC | 41 | PASS | Probe |
| `test_usb_standalone` | USB OTG FS | 5 | not run | Standalone |

### Hardware Test Evidence (2026-03-28)

| Subsystem | Test | Result | Details |
|---|---|---|---|
| LEDs (4x GPIO) | test_led | 16/16 | Individual, all-on/off, rapid toggle, by-color |
| SDRAM (16MB FMC) | test_sdram | 14/14 | Init, checkerboard, inverse, address, random, boundary |
| SDRAM (exhaustive) | test_sdram_full | 13/13 | Walking bits, March C, multi-pass random, byte/halfword-level |
| GPIO + Button | test_gpio | 5/5 | PA0 input, multi-port output |
| UART (USART1) | test_uart | 4/4 | Init, byte TX, formatted, multi-byte (nb::block!) |
| Timers | test_timers | 8/8 | TIM2 1ms delay, TIM3 50ms delay, PWM, cancel |
| DMA | test_dma | 4/4 | 64B, 4096B, repeated mem-to-mem transfers |
| LCD (DSI/LTDC) | test_lcd | 13/13 | SDRAM framebuf, LTDC init, DSI init, OTM8009A, RGB fills |
| RNG | test_all (rng) | 3/3 | Non-zero, uniqueness, consecutive differ |
| ADC temp sensor | test_all (adc) | 2/2 | Temperature and Vrefint reads |
| All-in-one | test_all | 41/41 | Single flash, Peripherals::steal() between suites |
| USB CDC | test_usb_standalone | — | Builds clean; requires st-flash + USB cable (not run) |

### How probe-rs output works
Tests output via `defmt` over RTT to the ST-Link debug probe. `probe-rs run`
captures this and prints to the terminal. The format is parseable:

    TEST <name>: PASS
    TEST <name>: FAIL <reason>
    SUMMARY: X/Y passed
    ALL TESTS PASSED

The runner script (`run_tests.sh`) detects `SUMMARY` in the RTT output,
kills probe-rs (since the target loops forever), and parses pass/fail counts.

Credits
-------
Thanks to the authors of [stm32f429i-disc](https://github.com/stm32-rs/stm32f429i-disc.git) and [stm32f407g-disc](https://github.com/stm32-rs/stm32f407g-disc.git) crates for solid starting points.

License
-------

[0-clause BSD license](LICENSE-0BSD.txt).
