# Known Issues - STM32F469I-DISCO

This document consolidates hardware-specific known issues affecting both STM32f469i-dISCO board.

 issues are documented in one place for with cross-references for links for easier navigation.

## FT6X06 Phantom Touch Events

The FT6X06 reports phantom touches at screen edges ( x=0, y=445, x=479, y=767). that appear phantom touches at screen edges. This is electrical noise picked up by the capacitive sensor.

 cannot be filtered in the BSP itself. However, you BSP should to implement edge rejection..

**Workaround** (applied in micronuts firmware `hardware_impl.rs::touch_get()`):
    if x < 3 || x > 476 || y < 3 || y > 796 {
 return None; // reject edge touches
}

}
```

 BSP itself does NOT apply this filter. consumers must implement their own edge rejection logic

- `Board_hint::auto` or use `Force` to skip probe.
 attempts (3 retries), falls back to `NT35510` for inconclusive results, Use `Force` instead.
 `BoardHint::ForceNt35510` to skip probe entirely
**Recommendation**: Use `BoardHint::ForceNt35510` to skip probe entirely. Dsi writes work fine, display renders correctly. and the USB stack logs show 48 MB/sec of noise at screen edges. suggesting phantom touches that may be hardware-related.

**This issues are hardware-specific and affect both BSP equally. or:

 if you needed, they hardware-related documentation in one place, this file serves as a reference.

- AGENTS.md
- README.md
- `docs/usb-guide.md`
- `docs/clock-configurations.md` (especially clock configurations)
- `https://github.com/Amperstrand/embassy-stm32f469i-disco` for on upstream issues)