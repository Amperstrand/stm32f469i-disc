# stm32f469i-disc

Board support crate for the STM32F469I-DISCOVERY kit. An async version using Embassy is available at [embassy-stm32f469i-disco](https://github.com/Amperstrand/embassy-stm32f469i-disco). Plan is to upstream fixes and improvements to stm32f4xx-hal and related crates once testing is complete.

## Status: Active

This BSP is actively developed and tested. An Embassy async version exists at `embassy-stm32f469i-disco` for async use cases. Upstream contributions to `stm32f4xx-hal` and related crates are planned once hardware testing is complete and all decisions are finalized.

## Known-Good Pins

| Commit | Notes |
|--------|-------|
| `e9b96f8` (main HEAD) | HAL bump 05d999d, step logging, hw_diag chip ID fix, hw_diag device verified 24/24 |
| `93bbf39` | HAL bump 05d999d, init_panel/init_display_full step logging |
| `85c50eb` | BoardHint::Auto + probe_board_revision() I2C detection |
| `d12977e` | Comprehensive audit: bugs, docs, code quality, metadata |
| `5290ae7` | USB standalone test, RNG/ADC tests, host companion |
| `da9fdb2` | LTDC init refactors, double-framebuffer merge, ForceNt35510 hint |

## Hardware Test Evidence

All testing performed on STM32F469I-Discovery board (B08 revision, NT35510 panel).

### Test Date: 2026-03-30 (post-bugfix re-verification, stm32f4xx-hal 05d999d)

Automated test suite using probe-rs + defmt/RTT. All tests use blocking `stm32f4xx-hal` 0.23.

| Subsystem | Status | Evidence | Tests |
|-----------|--------|----------|-------|
| **LEDs (GPIO)** | PASS | test_led: 16/16 | Individual, all-on/off, rapid toggle, by-color |
| **SDRAM (fast)** | PASS | test_sdram: 10/10 | Checkerboard, address, random, walking-1s, March C, boundary, scattered, end-of-RAM, byte, halfword |
| **SDRAM (exhaustive)** | PASS | test_sdram_full: 13/13 | Walking bits, March C, multi-pass random, byte/halfword |
| **GPIO + Button** | PASS | test_gpio: 5/5 | PA0 input, multi-port output |
| **UART (USART1)** | PASS | test_uart: 4/4 | Init, byte TX, formatted, multi-byte |
| **Timers** | PASS | test_timers: 8/8 | TIM2 1ms, TIM3 50ms, PWM, cancel |
| **DMA** | PASS | test_dma: 5/5 | 64B, 4096B, repeated mem-to-mem, DWT timing check |
| **LCD (DSI/LTDC)** | PASS | test_lcd: 13/13 | SDRAM framebuf, LTDC, DSI, OTM8009A, RGB fills |
| **Touch (FT6X06)** | PASS | test_touch: 5/5 | I2C init, chip ID (0x11), FT6X06 init, TD status, interactive |
| **RNG** | PASS | test_all: 3/3 | Non-zero, uniqueness, consecutive differ |
| **ADC temp sensor** | PASS | test_all: 2/2 | Temperature and Vrefint |
| **All-in-one** | PASS | test_all: 44/44 | Single flash, all suites via Peripherals::steal() |
| **On-screen diag** | PASS | hw_diag: 24/24 | SDRAM, display fills/gradient/text, touch, GPIO, LEDs, timers |
| **Soak test** | BUILT | test_soak: builds clean | Continuous SDRAM stress; no SUMMARY line |
| **USB CDC** | BUILT | test_usb_standalone builds clean | Requires st-flash + USB cable; not run |

### Test Date: 2026-03-25/26

Testing performed during the micronuts porting project (before migration to Embassy).

| Subsystem | Status | Evidence | Notes |
|-----------|--------|----------|-------|
| **SDRAM** | PASS | fmc_sdram_test example verified | 16MB FMC write/read |
| **Display (DSI/LTDC)** | PASS | display_dsi_lcd and display_hello_eg work | NT35510 via DSI, auto-detected panel |
| **Touch (FT6X06)** | PASS | display_touch example works | I2C1, PB8/PB9. LCD panel power (PH7) required before I2C. Phantom touches filtered. |
| **GPIO/LEDs** | PASS | gpio_hal_blinky works | User LEDs cycle correctly |
| **USB CDC** | PASS | usb_cdc_serial echo test works | OTG FS, st-link hal |
| **DSI reads** | FAIL | Probe fails (3/3 retries) | Workaround: skip probe, use ForceNt35510 or known panel type |
| **SDIO** | NOT TESTED | sdio_raw_test exists but not verified on this board | Out of scope for wallet use case |

## Known Issues (all closed)



## Cargo.toml defmt feature gate (FIXED)

**Bug:** `"defmt"` was unconditionally present in `stm32f4xx-hal` features (line 54 of Cargo.toml), making the `defmt` feature gate on line 70 useless. Downstream consumers using `default-features = false` without the `defmt` feature still got defmt compiled into the HAL.

**Fix:** Removed `"defmt"` from the unconditional features list. The `defmt` feature gate (`defmt = ["dep:defmt", "stm32f4xx-hal/defmt"]`) still adds it when the feature is enabled.

**Impact:** Consumers using `default-features = false` without `defmt` now correctly build without defmt in the HAL. Consumers using the `defmt` feature are unaffected. See [issue #23](https://github.com/Amperstrand/stm32f469i-disc/issues/23).

## Key Dependencies

- `stm32f4xx-hal` @ `05d999d` (Amperstrand fork — DSI host, PLLSAI fix)
- `otm8009a` @ `76dcda9` (Amperstrand fork — eh 1.0, edition 2024)
- `nt35510` @ `7d588ef` (Amperstrand fork)
- `ft6x06-rs` @ `fa4b41c` (Amperstrand fork of DogeDark/ft6x06-rs)
- `stm32-fmc` 0.4.0 (SDRAM)
- `embedded-hal` 1.0 (BSP is pure eh 1.0; eh 0.2 exists only as transitive dep of stm32f4xx-hal)

## Migration Notes

An async version using Embassy is available at `embassy-stm32f469i-disco`, which provides the same peripherals (SDRAM, DSI/LTDC/NT35510, FT6X06) and has fixed DSI read issues (RawDsi::read() FIFO flow control). All hardware test evidence above applies to the Embassy BSP as well.

## Upstream Interaction Policy

**NEVER file PRs or issues on upstream projects (stm32-rs, embassy-rs, DougAnderson444, etc.) without human review and approval.** AI-generated bug diagnoses can be confidently wrong. If you find a potential upstream bug:
1. Document your findings in an Amperstrand repo issue first
2. Include all evidence (register dumps, test results, methodology)
3. Let a human decide whether to escalate

This repo is a fork of an upstream BSP. Changes intended for upstream stm32-rs/stm32f4xx-hal or related projects must go through human review first.

See [Amperstrand/micronuts#19](https://github.com/Amperstrand/micronuts/issues/19) for a retrospective on how a confident misdiagnosis wasted upstream maintainer time.

## Hardware Testing Checklist

Tests that require the STM32F469I-DISCO board connected via ST-Link.

### Quick re-verification (~2 min)
```bash
./run_tests.sh all
```
Runs: test_led, test_sdram, test_gpio, test_uart, test_timers, test_dma, test_lcd, test_touch, test_all, hw_diag (10 suites, ~137 tests).

### Explicit-only tests (not in `run_tests.sh all`)

| Test | Command | Duration | What it verifies |
|------|---------|----------|-----------------|
| `test_sdram_full` | `./run_tests.sh test_sdram_full` | ~3-5 min | Exhaustive 16MB SDRAM: walking bits, March C, multi-pass random |
| `test_soak` | `./run_tests.sh test_soak` | Hours (kill manually) | Continuous rotating patterns + bit fade (DRAM retention) on full 16MB |
| `test_usb_standalone` | `st-flash write target/.../test_usb_standalone.bin 0x08000000` | ~30s | USB CDC echo — requires st-flash + USB cable, NOT probe-rs (breaks USB timing) |

### Post-change verification triggers
- After HAL fork bump: `./run_tests.sh all` (full re-verify)
- After lcd.rs changes: `./run_tests.sh test_lcd test_all hw_diag`
- After touch.rs changes: `./run_tests.sh test_touch test_all hw_diag`
- After sdram.rs changes: `./run_tests.sh test_sdram test_all hw_diag`
- After any src/ change: `make check` first (software-only), then `./run_tests.sh all`

### USB testing procedure
1. Build: `cargo build --release --example test_usb_standalone --target thumbv7em-none-eabihf`
2. Flash with st-flash (NOT probe-rs): `st-flash write target/thumbv7em-none-eabihf/release/examples/test_usb_standalone.bin 0x08000000`
3. Connect USB cable from board's USB OTG FS port to host PC
4. Run host companion: `python3 tests/host/test_usb_host.py --device /dev/ttyACM0 --duration 60`
5. Or use: `scripts/usb_test.sh`

**Why not probe-rs for USB?** probe-rs halts the CPU periodically for RTT reads, breaking USB timing. The USB test must run standalone with no debug probe interference.
