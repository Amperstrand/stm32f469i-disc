# Hardware test plan

Run each BSP example on the STM32F469I-DISCO board and record pass/fail. Use this runbook to confirm that the touch screen works, the SD card works, and that the examples work in general.

## Prerequisites

- **Board**: STM32F469I-DISCO (Discovery kit)
- **Probe**: ST-Link (on-board or external) for flashing and RTT
- **USB**: Cable for probe (and for USB CDC example)
- **SD tests**: A microSD card inserted in the on-board slot (for `sdio_raw_test` and optionally `sdio_speed_sweep`)
- **Display/touch**: Board’s LCD and touch panel (for display and touch examples)
- **Host**: Either local machine with probe-rs, or remote host (e.g. Ubuntu) with probe-rs; see “How to run” below

## How to run

- **Local** (board and probe connected to this machine):  
  `cargo run --example <name> [--features ...]`  
  Observe: LEDs, LCD, defmt/RTT log, or host serial (USB CDC).

- **Remote** (e.g. build here, SCP ELF, run on Ubuntu with probe):  
  Copy the built ELF to the remote host and run probe-rs there. Ensure the board is connected to that host.

Record the date and Pass/Fail (and any notes) in the table below.

## Confirming tests when RTT is not in logs

When `CAPTURE_RTT=1` only captures probe-rs stdout (no defmt), confirm SDRAM, SD, and USB as follows:

1. **Deploy the example** (from your machine): copy the built ELF to the remote host. The script will block while probe-rs runs.

2. **On the remote host**, run probe-rs by hand to see defmt in the live terminal:
   - `pkill -9 -f probe-rs; sleep 2`
   - `DEFMT_LOG is set in .cargo/config.toml /home/ubuntu/.local/bin/probe-rs run --chip STM32F469NIHx --rtt-scan-memory /tmp/<example>`
   - Replace `<example>` with `fmc_sdram_test`, `sdio_raw_test`, `usb_cdc_serial`, or `sdio_speed_sweep`. Watch for defmt (e.g. "Initializing SDRAM...", "PASS", "SDIO test done", speed sweep results). Ctrl+C when done.

3. **USB CDC** and **SD** can also be confirmed by observation: USB — host sees a new serial port and echo works; SD — board does not reset with a card in the slot.

## Test run record (systematic run)

| # | Example | Date run | Result | Notes |
|---|---------|----------|--------|-------|
| 1 | gpio_hal_blinky | 2026-03-07 | Pass | LEDs cycle (green, orange, red, blue). |
| 2 | fmc_sdram_test | 2026-03-08 | Run | Re-ran with CAPTURE_RTT=1 RTT_CAPTURE_SEC=45. Log has only probe-rs output. Confirm: run probe-rs interactively on remote or observe board does not reset. |
| 3 | display_dsi_lcd | 2026-03-07 | Pass | Rolling gradient on LCD. |
| 4 | display_hello_eg | 2026-03-07 | Pass | Text and shapes on LCD. |
| 5 | display_touch | 2026-03-07 | Pass | Paint demo: touch draws, coords, clear button — "worked really well." |
| 6 | sdio_raw_test | 2026-03-08 | Run | Re-ran with CAPTURE_RTT=1 RTT_CAPTURE_SEC=90. Log has only probe-rs output. Confirm: microSD in slot, run probe-rs interactively for "PASS" or observe no panic. |
| 7 | usb_cdc_serial | 2026-03-08 | Run | Re-ran with CAPTURE_RTT=1 RTT_CAPTURE_SEC=25. Confirm: connect board USB device port to host; serial port appears; echo works. |
| 8 | sdio_speed_sweep | 2026-03-08 | Run | Re-ran with CAPTURE_RTT=1 RTT_CAPTURE_SEC=90. Log has only probe-rs output. Confirm: run probe-rs interactively for sweep results + RECOMMENDATION, or observe no panic with SD in slot. |

**SD card:** Both **sdio_raw_test** (10 MiB at 1 MHz) and **sdio_speed_sweep** (1/4/8/12/24 MHz) were run again 2026-03-08. Defmt does not appear in captured logs; use interactive probe-rs on the remote or board observation to confirm pass/fail. **To record observed results** (including speed sweep per-frequency outcomes), add a row to [docs/SDIO-CLOCK-SPEEDS.md](docs/SDIO-CLOCK-SPEEDS.md).

## Ordered checklist

Run in this order to isolate failures (e.g. probe first, then SDRAM, then display, then touch, then SD, then USB).

| # | Example | Purpose | Command | Expected | Record (Date / Pass / Fail / Notes) |
|---|---------|---------|---------|----------|-------------------------------------|
| 1 | gpio_hal_blinky | Probe + LEDs | `./run_tests.sh` or `cargo run --example gpio_hal_blinky` | LEDs cycle (green, orange, red, blue) | |
| 2 | fmc_sdram_test | SDRAM | `./run_tests.sh` or `cargo run --example fmc_sdram_test` | RTT/log: PASS (pattern verification) | |
| 3 | display_dsi_lcd | Display | `./run_tests.sh` or `cargo run --example display_dsi_lcd` | Rolling gradient on LCD | |
| 4 | display_hello_eg | Display + embedded-graphics | `./run_tests.sh` (script uses framebuffer) or `cargo run --example display_hello_eg --features framebuffer` | Text and shapes on LCD | |
| 5 | display_touch | Touch | `./run_tests.sh` or `cargo run --example display_touch` | Touch/swipe detected; feedback on screen | |
| 6 | sdio_raw_test | SD card | `./run_tests.sh` or `cargo run --example sdio_raw_test` | RTT: SD init + 10 MiB read PASS | |
| 7 | usb_cdc_serial | USB CDC | `./run_tests.sh` or `cargo run --example usb_cdc_serial --features usb_fs` | Host sees serial port; echo works | |
| 8 | sdio_speed_sweep | SD speeds (**optional**) | `./run_tests.sh` or `cargo run --example sdio_speed_sweep --features sdio-speed-test` | RTT: sweep results + RECOMMENDATION + TRADEOFFS | |

**Optional (8)**: The SD speed sweep is for card characterization. You can skip it when validating “examples just work.” Run it only when you want to document which clock speeds work on your card; see [SDIO clock speeds](SDIO-CLOCK-SPEEDS.md).

## How to use this runbook

1. Run the examples in the order above (1–7 for core validation; 8 optional).
2. For each row, note the date you ran it and whether it **Pass** or **Fail** (and any short notes, e.g. “timeout at 12 MHz”).
3. If something fails, record which example and what you saw (RTT message, display behavior, etc.) so we can fix or document it.
4. When all required examples pass, the board support is validated for LEDs, SDRAM, display, touch, SD card, and USB CDC.
