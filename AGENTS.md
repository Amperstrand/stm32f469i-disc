# stm32f469i-disc

Board support crate for the STM32F469I-DISCOVERY kit. **Legacy / maintenance mode** — new development uses [embassy-stm32f469i-disco](https://github.com/Amperstrand/embassy-stm32f469i-disco) (Embassy async BSP).

## Status: Maintenance Mode

This BSP is no longer actively developed. The micronuts firmware has been ported to the Embassy async framework using `embassy-stm32f469i-disco`. This crate remains available for reference and for the gm65-scanner standalone firmware which still uses the sync `stm32f4xx-hal`.

## Known-Good Pin

| Commit | Notes |
|--------|-------|
| `da9fdb2` (main HEAD) | LTDC init refactors, double-framebuffer merge, ForceNt35510 hint |

## Hardware Test Evidence

All testing performed on STM32F469I-Discovery board (B08 revision, NT35510 panel).

### Test Date: 2026-03-25/26

Testing performed during the micronuts porting project (before migration to Embassy).

| Subsystem | Status | Evidence | Notes |
|-----------|--------|----------|-------|
| **SDRAM** | PASS | fmc_sdram_test example verified | 16MB FMC write/read |
| **Display (DSI/LTDC)** | PASS | display_dsi_lcd and display_hello_eg work | NT35510 via DSI, auto-detected panel |
| **Touch (FT6X06)** | PASS | display_touch example works | I2C1, PB8/PB9. Phantom touches at edges (known) |
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
- `nt35510` @ `7d588ef` (Amperstrand fork)
- `ft6x06` @ `2ed36f7` (Amperstrand fork)
- `stm32-fmc` 0.4 (SDRAM)
- `embedded-hal` 1.0 + 0.2 (dual trait support)

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
