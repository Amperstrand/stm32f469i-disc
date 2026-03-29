# stm32f469i-disc

Board support crate for the STM32F469I-DISCOVERY kit. **Legacy / maintenance mode** — new development uses [embassy-stm32f469i-disco](https://github.com/Amperstrand/embassy-stm32f469i-disco) (Embassy async BSP).

## Status: Maintenance Mode

This BSP is no longer actively developed. The micronuts firmware has been ported to the Embassy async framework using `embassy-stm32f469i-disco`. This crate remains available for reference and for the gm65-scanner standalone firmware which still uses the sync `stm32f4xx-hal`.

## Known-Good Pin

| Commit | Notes |
|--------|-------|
| `5290ae7` (main HEAD) | USB standalone test, RNG/ADC tests, host companion |
| `da9fdb2` | LTDC init refactors, double-framebuffer merge, ForceNt35510 hint |

## Hardware Test Evidence

All testing performed on STM32F469I-Discovery board (B08 revision, NT35510 panel).

### Test Date: 2026-03-28 (comprehensive BSP test suite)

Automated test suite using probe-rs + defmt/RTT. All tests use blocking `stm32f4xx-hal` 0.23.

| Subsystem | Status | Evidence | Tests |
|-----------|--------|----------|-------|
| **LEDs (GPIO)** | PASS | test_led: 16/16 | Individual, all-on/off, rapid toggle, by-color |
| **SDRAM (fast)** | PASS | test_sdram: 14/14 | Init, checkerboard, inverse, address, random, boundary |
| **SDRAM (exhaustive)** | PASS | test_sdram_full: 13/13 | Walking bits, March C, multi-pass random, byte/halfword |
| **GPIO + Button** | PASS | test_gpio: 5/5 | PA0 input, multi-port output |
| **UART (USART1)** | PASS | test_uart: 4/4 | Init, byte TX, formatted, multi-byte |
| **Timers** | PASS | test_timers: 8/8 | TIM2 1ms, TIM3 50ms, PWM, cancel |
| **DMA** | PASS | test_dma: 5/5 | 64B, 4096B, repeated mem-to-mem, DWT timing check |
| **LCD (DSI/LTDC)** | PASS | test_lcd: 13/13 | SDRAM framebuf, LTDC, DSI, OTM8009A, RGB fills |
| **Touch (FT6X06)** | PASS | test_touch: 5/5 | I2C init, chip ID, FT6X06 init, TD status, interactive |
| **RNG** | PASS | test_all: 3/3 | Non-zero, uniqueness, consecutive differ |
| **ADC temp sensor** | PASS | test_all: 2/2 | Temperature and Vrefint |
| **All-in-one** | PASS | test_all: 44/44 | Single flash, all suites via Peripherals::steal() |
| **On-screen diag** | BUILT | hw_diag: builds clean | Not run on device; requires st-link + display |
| **Soak test** | PASS | test_soak: builds clean | Continuous SDRAM stress; no SUMMARY line |
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

| Issue | Status | Resolution |
|-------|--------|------------|
| DSI probe reads fail (#12) | CLOSED | Low impact — ForceNt35510 workaround, writes work fine |
| SDIO microSD untested (#13) | CLOSED | Out of scope for wallet |
| SDIO speed sweep (#6) | CLOSED | Out of scope for wallet |
| FT6X06 phantom touches (#17) | CLOSED | 3px edge margin filter in firmware |
| defmt log level (#15) | CLOSED | Set DEFMT_LOG=info in firmware config |

## Open Issues (documentation/architecture only)

| Issue | Description |
|-------|-------------|
| #16 | USART6 TX/RX (PG14/PG9) not documented in PIN-CONSUMPTION.md |
| #14 | HIL test suite design not finalized |
| #10 | Architecture & test matrix tracking |
| #8 | probe-rs RTT race: defmt output lost on small binaries |
| #7 | Architecture decisions and upstream roadmap |
| #5 | ft6x06 dependency: evaluate DogeDark/ft6x06-rs replacement |

## Key Dependencies

- `stm32f4xx-hal` @ `b72958b` (Amperstrand fork — DSI host, PLLSAI fix)
- `otm8009a` @ `76dcda9` (Amperstrand fork — eh 1.0, edition 2024)
- `nt35510` @ `7d588ef` (Amperstrand fork)
- `ft6x06-rs` @ `fa4b41c` (Amperstrand fork of DogeDark/ft6x06-rs)
- `stm32-fmc` 0.4 (SDRAM)
- `embedded-hal` 1.0 (BSP is pure eh 1.0; eh 0.2 exists only as transitive dep of stm32f4xx-hal)

## Migration Notes

If you are starting a new project on STM32F469I-Discovery:
1. Use `embassy-stm32f469i-disco` instead of this crate
2. The Embassy BSP provides the same peripherals (SDRAM, DSI/LTDC/NT35510, FT6X06)
3. All hardware test evidence above applies to the Embassy BSP as well
4. The Embassy BSP has fixed DSI read issues (RawDsi::read() FIFO flow control)

## Upstream Interaction Policy

**NEVER file PRs or issues on upstream projects (stm32-rs, embassy-rs, DougAnderson444, etc.) without human review and approval.** AI-generated bug diagnoses can be confidently wrong. If you find a potential upstream bug:
1. Document your findings in an Amperstrand repo issue first
2. Include all evidence (register dumps, test results, methodology)
3. Let a human decide whether to escalate

This repo is a fork of an upstream BSP. Changes intended for upstream stm32-rs/stm32f4xx-hal or related projects must go through human review first.

See [Amperstrand/micronuts#19](https://github.com/Amperstrand/micronuts/issues/19) for a retrospective on how a confident misdiagnosis wasted upstream maintainer time.
