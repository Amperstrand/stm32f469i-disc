CHIP       ?= STM32F469NIHx
TARGET     := thumbv7em-none-eabihf
CARGO      ?= cargo
PROBE_RS   ?= probe-rs
RTT_FLAGS  := --rtt-scan-memory
TIMEOUT    ?= 120
LOG_DIR    := logs

# Core HIL tests (fast, no optional hardware required)
HIL_CORE := \
	gpio_hal_blinky \
	fmc_sdram_test \
	display_dsi_lcd \
	display_hello_eg \
	display_touch \
	sdio_raw_test \
	usb_cdc_serial

# Extended tests (slow, optional hardware)
HIL_EXTENDED := sdio_speed_sweep

HIL_ALL := $(HIL_CORE) $(HIL_EXTENDED)

# Per-example feature flags
FEATURES_display_hello_eg  := --features framebuffer
FEATURES_usb_cdc_serial    := --features usb_fs
FEATURES_sdio_speed_sweep  := --features sdio-speed-test

# ============================================================
# Top-level targets
# ============================================================

.PHONY: hiltest hiltest-extended hiltest-all clean-logs

hiltest: $(HIL_CORE)
	@echo ""
	@echo "=== HIL TEST SUMMARY ==="
	@for t in $(HIL_CORE); do \
		grep -q "HIL_RESULT:.*:PASS" /tmp/hil_$${t}.log 2>/dev/null && \
			echo "  $$t: PASS" || \
		(grep -q "HIL_RESULT:.*:SKIP" /tmp/hil_$${t}.log 2>/dev/null && \
			echo "  $$t: SKIP" || \
			echo "  $$t: FAIL"); \
	done

hiltest-extended: hiltest $(HIL_EXTENDED)
	@echo ""
	@echo "=== FULL HIL TEST SUMMARY ==="
	@for t in $(HIL_ALL); do \
		grep -q "HIL_RESULT:.*:PASS" /tmp/hil_$${t}.log 2>/dev/null && \
			echo "  $$t: PASS" || \
		(grep -q "HIL_RESULT:.*:SKIP" /tmp/hil_$${t}.log 2>/dev/null && \
			echo "  $$t: SKIP" || \
			echo "  $$t: FAIL"); \
	done

hiltest-all: hiltest-extended

clean-logs:
	rm -f /tmp/hil_*.log

# ============================================================
# Per-example build + flash + run + grep
# ============================================================

# Pattern rule for all HIL tests
# Usage: make gpio_hal_blinky  (or any example name)
define HIL_RULE
.PHONY: $(1)
$(1): build-$(1) run-$(1)
endef

$(foreach t,$(HIL_ALL),$(eval $(call HIL_RULE,$(t))))

# Build pattern
define BUILD_RULE
.PHONY: build-$(1)
build-$(1):
	$$(CARGO) build --release --example $(1) $$(FEATURES_$(1))
endef

$(foreach t,$(HIL_ALL),$(eval $(call BUILD_RULE,$(t))))

# Run pattern: flash + capture RTT + grep result
define RUN_RULE
.PHONY: run-$(1)
run-$(1): build-$(1)
	@mkdir -p $(LOG_DIR)
	timeout $(TIMEOUT) $(PROBE_RS) run --chip $(CHIP) $(RTT_FLAGS) \
		target/$(TARGET)/release/examples/$(1) \
		2>&1 | tee /tmp/hil_$(1).log || true
	@grep -q "HIL_RESULT:.*:PASS" /tmp/hil_$(1).log 2>/dev/null && \
		echo ">>> $(1): PASS" || \
	(grep -q "HIL_RESULT:.*:SKIP" /tmp/hil_$(1).log 2>/dev/null && \
		echo ">>> $(1): SKIP" || \
		echo ">>> $(1): FAIL")
	@cp /tmp/hil_$(1).log $(LOG_DIR)/ 2>/dev/null || true
endef

$(foreach t,$(HIL_ALL),$(eval $(call RUN_RULE,$(t))))

# ============================================================
# Convenience targets
# ============================================================

.PHONY: build-all
build-all: $(foreach t,$(HIL_ALL),build-$(t))

.PHONY: list
list:
	@echo "Core HIL tests:"
	@for t in $(HIL_CORE); do echo "  $$t"; done
	@echo "Extended HIL tests:"
	@for t in $(HIL_EXTENDED); do echo "  $$t"; done
