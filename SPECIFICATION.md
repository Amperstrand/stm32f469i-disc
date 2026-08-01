# STM32F469I-DISCO BSP Specification

Key behaviors extracted from the STM32F469 reference manual (RM0386) and
the STM32F469I-DISCO user manual (UM1932). Each section documents a
hardware behavior that the BSP implements.

## DSI Host Controller (RM0386 §14)

The DSI host controller MUST be initialized before enabling the LTDC
display controller. The DSI clock lane MUST be in stop state before
configuring the PLL.

The DSI host supports both video mode and command mode. For the
F469I-DISCO display, video mode is used with an NT35510 or OTM8009A
panel controller auto-detected via DSI read commands.

## LTDC Display Controller (RM0386 §13)

The LTDC pixel clock MUST be derived from PLLSAI_R. The default
configuration uses PLLSAI_N=384, PLLSAI_R=7, giving a 54.86 MHz
pixel clock for the 480x800 DSI display.

The framebuffer MUST reside in SDRAM (0xC0000000) accessed via FMC.
The LTDC layer configuration MUST match the DSI color coding.

## FMC SDRAM Controller (RM0386 §9.3)

The FMC Bank 1 is used for SDRAM. The SDRAM MUST be configured with:
- 4 banks, 4096 rows, 256 columns, 16-bit data bus
- CAS latency 3 cycles
- 8-word burst length
- Auto-refresh enabled

SDRAM base address: 0xC0000000. Size: 16 MB (IS42S32400F-6BL).

## SDIO (RM0386 §10)

The SDIO peripheral MUST initialize at 400 kHz (F400Khz) for card
detection, then switch to high speed. The BSP defaults to 1 MHz for
reliability with SDXC cards.

The SDIO clock lane MUST be configured before issuing CMD0 (GO_IDLE).

## I2C1 Touch Controller (RM0386 §24)

I2C1 on PB8(SCL)/PB9(SDA) at 400 kHz connects to the FT6X06 touch
controller. The FT6X06 interrupt line is on PC1.
