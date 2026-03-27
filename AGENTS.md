# stm32f469i-disc

Board support package for the STM32F469I-DISCOVERY kit. Provides LCD, SDRAM, touch, SDIO, USB, LED, and button support built on stm32f4xx-hal.

## Build

```bash
cargo build
cargo build --example gpio_hal_blinky
```

## Run Examples

```bash
probe-rs run --chip STM32F469NIHx --example gpio_hal_blinky
probe-rs run --chip STM32F469NIHx --example sdram_test
```

## Architecture

```
src/
├── lib.rs     — Module declarations, board config
├── lcd.rs     — Display with auto-detection (DSI/LTDC, OTM8009A or NT35510)
├── led.rs     — On-board LEDs
├── sdram.rs   — 16MB SDRAM via FMC
├── touch.rs   — FT6X06 capacitive touch
├── sdio.rs    — SD card via SDIO
├── button.rs  — User button
└── usb.rs     — USB OTG FS
```

## Hardware

- MCU: STM32F469NIH6 (ARM Cortex-M4F, 180MHz)
- Display: 480x800 RGB565 LCD via DSI/LTDC (NT35510 or OTM8009A controller)
- SDRAM: 16MB via FMC (IS42S32400F-6BL)
- Touch: FT6X06 capacitive touch via I2C
- SD card via SDIO
- USB OTG FS

## Key Dependencies

- `stm32f4xx-hal` (Amperstrand fork) — HAL with DSI/LTDC/SDRAM support
- `nt35510` (Amperstrand fork) — DSI display controller
- `ft6x06` (Amperstrand fork) — Touch controller
- `stm32-fmc` 0.4 — SDRAM controller

## Upstream Interaction Policy

**NEVER file PRs or issues on upstream projects (stm32-rs, embassy-rs, DougAnderson444, etc.) without human review and approval.** AI-generated bug diagnoses can be confidently wrong. If you find a potential upstream bug:
1. Document your findings in an Amperstrand repo issue first
2. Include all evidence (register dumps, test results, methodology)
3. Let a human decide whether to escalate

This repo is a fork of an upstream BSP. Changes intended for upstream stm32-rs/stm32f4xx-hal or related projects must go through human review first.

See [Amperstrand/micronuts#19](https://github.com/Amperstrand/micronuts/issues/19) for a retrospective on how a confident misdiagnosis wasted upstream maintainer time.
