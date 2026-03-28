# Display Pixel Format Reference

## Hardware Chain

The STM32F469I-DISCO display pipeline is:

```
Framebuffer (SDRAM) → LTDC (LCD-TFT Controller) → DSI Host → MIPI DSI → Panel (NT35510/OTM8009A)
```

Each stage has its own pixel format configuration.

## Panel Capabilities (NT35510 / OTM8009A)

Both panels on this board support:
- **16-bit**: RGB565 (5-6-5 bits per channel)
- **18-bit**: RGB666 (6-6-6 bits, padded in DSI transport)
- **24-bit**: RGB888 (8-8-8 bits)

The panel receives pixel data via MIPI DSI. The DSI host's `ColorCoding` setting determines
how many bits per pixel are sent over the DSI link. The panel then maps this to its internal
display.

## LTDC Pixel Formats (stm32f4xx-hal)

The LTDC layer pixel format (`PixelFormat` enum) determines how it reads from the framebuffer
in SDRAM:

| Format        | Rust type | Bits/pixel | LTDC register | SDRAM bytes/pixel |
|---------------|-----------|------------|---------------|-------------------|
| `ARGB8888`    | `u32`     | 32         | `0b000`       | 4                 |
| `RGB888`      | —         | 24         | `0b001`       | — (not implemented in HAL) |
| `RGB565`      | `u16`     | 16         | `0b010`       | 2                 |
| `ARGB1555`    | `u16`     | 16         | `0b011`       | 2                 |
| `ARGB4444`    | `u16`     | 16         | `0b100`       | 2                 |
| `L8`          | `u8`      | 8          | `0b101`       | 1                 |
| `AL44`        | `u8`      | 8          | `0b110`       | 1                 |
| `AL88`        | `u16`     | 16         | `0b111`       | 2                 |

## DSI Color Coding

The DSI host `ColorCoding` enum controls bits per pixel on the MIPI DSI link:

| Variant             | Bits/pixel | Notes |
|---------------------|------------|-------|
| `SixteenBitsConfig1` | 16        | RGB565 |
| `SixteenBitsConfig2` | 16        | RGB565 (different packing) |
| `SixteenBitsConfig3` | 16        | RGB565 (different packing) |
| `EighteenBitsConfig1`| 18        | RGB666 |
| `EighteenBitsConfig2`| 18        | RGB666 (different packing) |
| `TwentyFourBits`     | 24        | RGB888 |

**The LTDC pixel format and DSI color coding are independent.** LTDC reads the framebuffer
in its configured format, and the DSI host transmits to the panel in its configured format.
The LTDC handles the conversion between the two.

## Framebuffer Memory Usage (480 x 800 panel)

| LTDC Format | Bytes/pixel | Framebuffer size | % of 16MB SDRAM | Remaining SDRAM |
|-------------|-------------|-----------------|-----------------|-----------------|
| `L8`        | 1           | 384 KB          | 2.3%            | 15.6 MB         |
| `RGB565`    | 2           | 768 KB          | 4.6%            | 15.2 MB         |
| `ARGB8888`  | 4           | 1,536 KB        | 9.2%            | 14.5 MB         |
| Double `L8`  | 1 x 2       | 768 KB          | 4.6%            | 15.2 MB         |
| Double `RGB565` | 2 x 2    | 1,536 KB        | 9.2%            | 14.5 MB         |
| Double `ARGB8888` | 4 x 2 | 3,072 KB       | 18.4%           | 12.9 MB         |

## What Each BSP Uses

### Sync BSP (`stm32f469i-disc`)

- **`lcd.rs` high-level API** (`init_display_full`, `init_framebuffer`): Uses **RGB565** (`u16`).
  The `LtdcFramebuffer<u16>` wrapper implements `embedded-graphics` `DrawTarget` with `Rgb565`.
- **`lcd.rs` low-level API** (`init_ltdc_argb8888`): Provides an `ARGB8888` (`u32`) option.
- **Test examples** (`test_lcd.rs`, `test_all.rs`, `hw_diag.rs`): Use **ARGB8888** (`u32`).
  These manually configure LTDC with `PixelFormat::ARGB8888` and DSI with `TwentyFourBits`.
  The framebuffer is a raw `&mut [u32]` — no `embedded-graphics` `DrawTarget` impl.

### Embassy BSP (`embassy-stm32f469i-disco`)

- **`DisplayCtrl`**: Uses **RGB565** (`u16`). The `FramebufferView` wrapper implements
  `embedded-graphics` `DrawTarget` with `Rgb565`. Direct LTDC register config writes `0x02`
  (RGB565) to the layer pixel format register. DSI uses `TwentyFourBits`.

## Tradeoffs

### RGB565 (Recommended for most use)

**Pros:**
- Half the framebuffer memory of ARGB8888 (768 KB vs 1,536 KB)
- Native `embedded-graphics` `Rgb565` color type with full API support
- `Rgb565::CSS_RED`, `Rgb565::GREEN`, `Rgb565::BLUE`, etc. work out of the box
- Direct `Rgb565::new(r, g, b)` construction
- `DrawTarget` impl is straightforward — each pixel is exactly `u16`
- Sufficient color depth for most UI/text/graphics (65,536 colors)
- Matches what both BSP high-level APIs use
- Less DMA2D bandwidth for blitting operations

**Cons:**
- No per-pixel alpha channel (no transparency)
- Color banding on gradients (only 32/64 shades of R/G/B)
- Visible posterization on smooth gradients and photos

### ARGB8888

**Pros:**
- Full 24-bit color (16.7M colors) — no visible banding
- Per-pixel alpha channel (8-bit)
- Better for photo viewing, smooth gradients
- Can leverage DMA2D alpha blending

**Cons:**
- Double the framebuffer memory (1,536 KB vs 768 KB)
- No native `embedded-graphics` `Argb8888` type (only `Rgb888` exists, no alpha)
- To use alpha, need custom pixel type + custom `DrawTarget` impl
- `Rgb888::CSS_RED` etc. are `Rgb888::CSS_RED` (not `Rgb888::RED`)
- More DMA2D bandwidth for blitting

### RGB666 (18-bit, DSI transport only)

**Pros:**
- Better color than RGB565 (262K colors) with less memory than ARGB8888
- Can use with LTDC `RGB565` format (DSI handles upscaling)

**Cons:**
- Not a native LTDC framebuffer format — must use LTDC RGB565 and DSI EighteenBits
- Color data is padded in DSI transport (18 bits → 24 bits on wire)
- No standard `embedded-graphics` color type for RGB666
- Minimal practical benefit over RGB565

## Recommendation

**Use RGB565 (`u16`) for all new code.** It's the right choice because:

1. **Memory**: 768 KB framebuffer leaves ~15 MB of SDRAM free for heap, textures, etc.
2. **Ecosystem**: `embedded-graphics` has first-class `Rgb565` support. All drawing primitives,
   fonts, and image formats work out of the box.
3. **BSP alignment**: Both BSPs' high-level APIs use RGB565. The embassy BSP's `FramebufferView`
   implements `DrawTarget<Rgb565>` directly.
4. **Performance**: Less memory bandwidth, faster fills and blits.

Use ARGB8888 only when you need per-pixel alpha blending or true 24-bit color depth,
and you're willing to implement a custom `DrawTarget` adapter (like `hw_diag.rs` does).

## embedded-graphics Color Type API (v0.8)

```rust
use embedded_graphics::pixelcolor::Rgb565;

// Construction
let red   = Rgb565::new(255, 0, 0);
let green = Rgb565::new(0, 255, 0);
let blue  = Rgb565::CSS_BLUE;      // = Rgb565::new(0, 0, 255)
let white = Rgb565::WHITE;          // = Rgb565::new(255, 255, 255)
let black = Rgb565::BLACK;          // = Rgb565::new(0, 0, 0)

// Raw access
let raw: u16 = red.into_storage();  // packed RGB565 value

// Conversion to other formats (for framebuffer)
let raw: u16 = Rgb565::new(255, 0, 0).into_storage();
```

```rust
use embedded_graphics::pixelcolor::Rgb888;

// Construction
let red = Rgb888::new(255, 0, 0);
let navy = Rgb888::CSS_NAVY;  // = Rgb888::new(0, 0, 128)

// NOTE: Rgb888 does NOT have .r(), .g(), .b() accessor methods
// Use into_storage() for raw u32 access (but packing is different from ARGB8888!)
let raw: u32 = red.into_storage();  // 0x00FF0000 (RGB packed into u32)

// For ARGB8888 framebuffer (what LTDC expects), manual conversion needed:
fn rgb888_to_argb8888(c: Rgb888) -> u32 {
    (0xFF << 24) | ((c.into_storage() & 0x00FF0000) as u32)
                | ((c.into_storage() & 0x0000FF00) as u32)
                | ((c.into_storage() & 0x000000FF) as u32)
}
```
