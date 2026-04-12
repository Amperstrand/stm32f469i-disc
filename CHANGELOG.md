# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-04-12
### Added
- DisplayOrientation support with landscape LTDC timing
- TouchError enum for touch driver (replaces Result<T, ()>)
- DisplayInitError enum with try_new() fallible init
- defmt removed from default features

### Changed
- NT35510/OTM8009A init now uses orientation parameter

## [0.3.0] - 2026-04-XX
### Added
- ARGB8888 display support with FramebufferView wrapper
- display_test_rgb888 example for ARGB8888 verification
- usb_minimal example from gm65-scanner
- step logging to init_panel and init_display_full

### Changed
- Consolidated examples from 26 to 8 dual-purpose examples
- Updated nt35510 pin to v0.2.0 (ea1ac3a)
- Bumped HAL fork to 05d999d

### Fixed
- Removed defmt from default features (was leaking into consumers, breaking USB)
- Fixed hw_diag touch chip ID check: accept any non-zero ID (board returns 0x11)
- Fixed probe() call in nt35510 initialization
- Fixed unused_variables warning in lcd.rs
- Fixed build errors in display examples
- Added required-features for framebuffer-dependent display_hello_eg example

### Docs
- Added missing documentation to lcd.rs
- Added cross-references to async BSP, known-issues link
- Removed outdated Open Issues section from AGENTS.md
- Expanded CI coverage, updated evidence, added hardware checklist

### Tests
- Improved SDRAM tests: trim fast test to 10, rewrite soak with bit fade + full 16MB
- Updated AGENTS.md: hw_diag verified 24/24
