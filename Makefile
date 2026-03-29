CHIP       ?= STM32F469NIHx
TARGET     := thumbv7em-none-eabihf
CARGO      ?= cargo

# probe-rs test suite (requires hardware)
PROBE_TESTS := test_led test_gpio test_uart test_timers test_dma \
               test_sdram test_sdram_full test_lcd test_touch test_all \
               hw_diag test_soak

# standalone tests (require st-flash + USB cable)
USB_TESTS   := test_usb_standalone

# feature-flagged examples
FB_EXAMPLES := test_lcd test_all hw_diag
USB_EXAMPLES := usb_cdc_serial test_usb_standalone

# ============================================================
# Software-only checks (no hardware)
# ============================================================

.PHONY: check clippy doc fmt build-all

check: build-all clippy doc
	@echo "=== All software checks passed ==="

build-all:
	$(CARGO) build --release --target $(TARGET) --lib
	$(CARGO) build --release --target $(TARGET) --lib --no-default-features
	$(CARGO) build --release --target $(TARGET) --lib --features framebuffer
	$(CARGO) build --release --target $(TARGET) --example test_led
	$(CARGO) build --release --target $(TARGET) --example test_gpio
	$(CARGO) build --release --target $(TARGET) --example test_uart
	$(CARGO) build --release --target $(TARGET) --example test_timers
	$(CARGO) build --release --target $(TARGET) --example test_dma
	$(CARGO) build --release --target $(TARGET) --example test_sdram
	$(CARGO) build --release --target $(TARGET) --example test_sdram_full
	$(CARGO) build --release --target $(TARGET) --example test_lcd
	$(CARGO) build --release --target $(TARGET) --example test_touch
	$(CARGO) build --release --target $(TARGET) --example test_all
	$(CARGO) build --release --target $(TARGET) --example test_soak
	$(CARGO) build --release --target $(TARGET) --example hw_diag
	$(CARGO) build --release --target $(TARGET) --example test_usb_standalone
	$(CARGO) build --release --target $(TARGET) --example defmt_hse_test
	$(CARGO) build --release --target $(TARGET) --example display_hello_eg --features framebuffer
	$(CARGO) build --release --target $(TARGET) --example usb_cdc_serial --features usb_fs

clippy:
	$(CARGO) clippy --release --target $(TARGET) -- -D warnings
	$(CARGO) clippy --release --target $(TARGET) --features framebuffer -- -D warnings
	@for ex in test_led test_gpio test_uart test_timers test_dma test_sdram test_sdram_full test_lcd test_touch test_all test_soak hw_diag test_usb_standalone defmt_hse_test; do \
		$(CARGO) clippy --release --target $(TARGET) --example "$$ex" -- -D warnings || exit 1; \
	done
	$(CARGO) clippy --release --target $(TARGET) --example display_hello_eg --features framebuffer -- -D warnings
	$(CARGO) clippy --release --target $(TARGET) --example usb_cdc_serial --features usb_fs -- -D warnings

doc:
	$(CARGO) doc --no-deps --target $(TARGET)
	$(CARGO) doc --no-deps --target $(TARGET) --features framebuffer,touch,defmt,rng

fmt:
	$(CARGO) fmt --all -- --check

fmt-fix:
	$(CARGO) fmt --all

# ============================================================
# Hardware tests (requires probe-rs + STM32F469I-DISCO)
# ============================================================

.PHONY: test probe-test usb-test

test: probe-test

probe-test:
	@echo "Running probe-rs test suite via run_tests.sh"
	./run_tests.sh all

usb-test:
	@echo "Running USB standalone test via scripts/usb_test.sh"
	./scripts/usb_test.sh

# ============================================================
# Convenience
# ============================================================

.PHONY: list clean

list:
	@echo "Software checks (no hardware):"
	@echo "  make check       Build + clippy + doc"
	@echo "  make build-all   Build lib + all examples"
	@echo "  make clippy      Lint with zero warnings"
	@echo "  make doc         Generate docs"
	@echo "  make fmt         Check formatting"
	@echo ""
	@echo "Hardware tests (requires board):"
	@echo "  make probe-test  Run probe-rs test suite (run_tests.sh)"
	@echo "  make usb-test    Run USB standalone test (st-flash + USB)"
	@echo ""
	@echo "Individual examples:"
	@for t in $(PROBE_TESTS) $(USB_TESTS); do echo "  $$t"; done

clean:
	$(CARGO) clean
