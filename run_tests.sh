#!/bin/bash
# BSP Test Runner - Flash, capture RTT, parse results
#
# Usage: ./run_tests.sh [test_name|all] [--no-flash]
#   test_name: test_led, test_sdram, test_lcd, test_gpio, or "all"
#   --no-flash: skip flashing (useful for re-running with already-flashed firmware)
#
# Requirements: probe-rs, cargo, an ARM target installed

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

CHIP="STM32F469NIHx"
PROBE_PROTOCOL="Swd"
TIMEOUT_DEFAULT=120
RTT_TIMEOUT=90

RESULTS_DIR="test-results"
mkdir -p "$RESULTS_DIR"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
REPORT_FILE="${RESULTS_DIR}/report_${TIMESTAMP}.txt"
LOG_DIR="${RESULTS_DIR}/logs_${TIMESTAMP}"
mkdir -p "$LOG_DIR"

TOTAL_SUITES=0
TOTAL_PASSED=0
TOTAL_FAILED=0
SUITE_RESULTS=()

flash_and_run() {
    local example=$1
    local timeout=${2:-$TIMEOUT_DEFAULT}
    local log_file="${LOG_DIR}/${example}.log"

    echo -e "${CYAN}==========================================${NC}"
    echo -e "${CYAN}  Running: ${example}${NC}"
    echo -e "${CYAN}==========================================${NC}"

    # Build
    echo -e "${YELLOW}Building ${example}...${NC}"
    if ! cargo build --release --example "$example" --target thumbv7em-none-eabihf 2>&1 | tee "${LOG_DIR}/${example}_build.log"; then
        echo -e "${RED}BUILD FAILED: ${example}${NC}"
        echo "BUILD FAILED: ${example}" >> "$REPORT_FILE"
        SUITE_RESULTS+=("BUILD_FAILED:${example}")
        TOTAL_SUITES=$((TOTAL_SUITES + 1))
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi
    echo -e "${GREEN}Build OK${NC}"

    # Flash and capture RTT output
    echo -e "${YELLOW}Flashing and running ${example} (timeout: ${timeout}s)...${NC}"

    local elf_path="target/thumbv7em-none-eabihf/release/examples/${example}"
    if [ ! -f "$elf_path" ]; then
        echo -e "${RED}ELF not found: ${elf_path}${NC}"
        echo "ELF NOT FOUND: ${elf_path}" >> "$REPORT_FILE"
        SUITE_RESULTS+=("FAIL:${example}")
        TOTAL_SUITES=$((TOTAL_SUITES + 1))
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    local rtt_output
    local exit_code=0

    timeout "${timeout}" probe-rs run \
        --chip "$CHIP" \
        --protocol "$PROBE_PROTOCOL" \
        "$elf_path" \
        > "$log_file" 2>&1 &
    local pid=$!

    # Wait for test to complete or timeout
    local waited=0
    local poll_interval=1
    while kill -0 "$pid" 2>/dev/null; do
        sleep $poll_interval
        waited=$((waited + poll_interval))
        if [ "$waited" -ge "$timeout" ]; then
            break
        fi
        # Check if tests finished (look for SUMMARY or ALL TESTS line)
        if grep -q "SUMMARY:\|ALL TESTS PASSED\|FAILED:" "$log_file" 2>/dev/null; then
            # Give it a moment for final output
            sleep 1
            kill "$pid" 2>/dev/null
            wait "$pid" 2>/dev/null
            break
        fi
    done

    # Ensure process is dead
    kill "$pid" 2>/dev/null
    wait "$pid" 2>/dev/null
    exit_code=$?
    parse_results "$example" "$log_file" "$exit_code"

    # Brief delay to release USB interface before next test
    sleep 3
    return 0
}

parse_results() {
    local example=$1
    local log_file=$2
    local exit_code=$3

    # Detect probe errors (USB busy, not found, etc.)
    if grep -qi "interface is busy\|Failed to open probe\|no probe found" "$log_file" 2>/dev/null; then
        echo -e "${RED}PROBE ERROR for ${example} - skipping${NC}"
        echo "PROBE_ERROR: ${example}" >> "$REPORT_FILE"
        SUITE_RESULTS+=("SKIP:${example}")
        TOTAL_SUITES=$((TOTAL_SUITES + 1))
        return 1
    fi

    # Show the log
    echo "--- RTT Output ---"
    cat "$log_file"
    echo "--- End RTT Output ---"
    echo ""

    local passed=0
    local failed=0
    local total=0
    local test_failures=""

    # Parse TEST <name>: PASS/FAIL from defmt output lines like:
    # [INFO ] TEST foo: PASS (file:line)
    # [ERROR] TEST foo: FAIL reason (file:line)
    while IFS= read -r line; do
        if echo "$line" | grep -qP 'TEST\s+\S+:\s+PASS'; then
            passed=$((passed + 1))
        elif echo "$line" | grep -qP 'TEST\s+\S+:\s+FAIL'; then
            failed=$((failed + 1))
            test_name=$(echo "$line" | grep -oP 'TEST\s+\K\S+')
            reason=$(echo "$line" | grep -oP 'FAIL\s+\K[^ (]+' || echo "unknown")
            test_failures="${test_failures}  - ${test_name}: ${reason}\n"
        fi
    done < "$log_file"

    total=$((passed + failed))

    # Also check for SUMMARY line like: SUMMARY: 16/16 passed
    summary_match=$(grep -oP 'SUMMARY:\s+\K(\d+)/(\d+)' "$log_file" 2>/dev/null || echo "")
    if [ -n "$summary_match" ]; then
        passed=$(echo "$summary_match" | cut -d'/' -f1)
        total=$(echo "$summary_match" | cut -d'/' -f2)
        failed=$((total - passed))
    fi

    # Check for ALL TESTS PASSED
    if grep -q "ALL TESTS PASSED" "$log_file" 2>/dev/null; then
        failed=0
    fi

    # Check for HardFault or panic
    if grep -qi "HardFault\|panicked\|panic\|exception" "$log_file" 2>/dev/null; then
        if [ $failed -eq 0 ]; then
            failed=1
            test_failures="${test_failures}  - CRASH: HardFault or panic detected\n"
        fi
    fi

    TOTAL_SUITES=$((TOTAL_SUITES + 1))

    echo ""
    echo -e "${CYAN}--- Results: ${example} ---${NC}"
    echo "  Passed: $passed"
    echo "  Failed: $failed"
    echo "  Total:  $total"

    if [ $failed -gt 0 ]; then
        echo -e "${RED}  Failures:${NC}"
        echo -e "$test_failures"
    fi

    # Write to report
    {
        echo ""
        echo "=========================================="
        echo "  Test Suite: ${example}"
        echo "=========================================="
        echo "  Passed: ${passed}/${total}"
        echo "  Exit code: ${exit_code}"
        if [ $failed -gt 0 ]; then
            echo "  STATUS: FAIL"
            echo -e "$test_failures"
        else
            echo "  STATUS: PASS"
        fi
    } >> "$REPORT_FILE"

    if [ $failed -eq 0 ]; then
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
        SUITE_RESULTS+=("PASS:${example}")
        echo -e "${GREEN}  >>> ${example}: PASSED <<<${NC}"
    else
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        SUITE_RESULTS+=("FAIL:${example}")
        echo -e "${RED}  >>> ${example}: FAILED <<<${NC}"
    fi
}

run_all() {
    local no_flash=${1:-false}

    echo "=========================================="
    echo "  STM32F469I-DISCO BSP Test Runner"
    echo "  $(date)"
    echo "=========================================="
    echo ""
    echo "Report: ${REPORT_FILE}"
    echo "Logs:   ${LOG_DIR}/"
    echo ""

    {
        echo "STM32F469I-DISCO BSP Test Report"
        echo "Date: $(date)"
        echo "Probe: $(probe-rs list 2>/dev/null | head -5 || echo 'not detected')"
        echo ""
    } > "$REPORT_FILE"

    # Check probe
    if ! probe-rs list 2>/dev/null | grep -qi "stm32\|probe"; then
        echo -e "${RED}ERROR: No debug probe detected${NC}"
        echo "Connect an ST-Link or J-Link probe and try again." >> "$REPORT_FILE"
        exit 2
    fi

    local tests=("test_led" "test_sdram" "test_gpio" "test_uart" "test_timers" "test_dma" "test_lcd")

    for test in "${tests[@]}"; do
        local timeout=$TIMEOUT_DEFAULT
        case "$test" in
            test_sdram) timeout=60 ;;
            test_lcd)   timeout=120 ;;
            test_gpio)  timeout=30 ;;
            test_led)   timeout=30 ;;
            test_uart)  timeout=30 ;;
            test_timers) timeout=30 ;;
            test_dma)   timeout=30 ;;
        esac

        flash_and_run "$test" "$timeout" || true
        echo ""
    done

    # Final report
    echo ""
    echo "=========================================="
    echo "  FINAL REPORT"
    echo "=========================================="
    echo ""

    {
        echo ""
        echo "=========================================="
        echo "  FINAL REPORT"
        echo "=========================================="
        echo ""
        echo "  Suites run:  ${TOTAL_SUITES}"
        echo "  Suites passed: ${TOTAL_PASSED}"
        echo "  Suites failed: ${TOTAL_FAILED}"
        echo ""
    } >> "$REPORT_FILE"

    for result in "${SUITE_RESULTS[@]}"; do
        status="${result%%:*}"
        name="${result#*:}"
        if [ "$status" = "PASS" ]; then
            echo -e "  ${GREEN}[PASS]${NC} ${name}"
            echo "  [PASS] ${name}" >> "$REPORT_FILE"
        else
            echo -e "  ${RED}[FAIL]${NC} ${name}"
            echo "  [FAIL] ${name}" >> "$REPORT_FILE"
        fi
    done

    echo ""
    echo "  Report saved to: ${REPORT_FILE}"
    echo "  Logs saved to:   ${LOG_DIR}/"
    echo "  Report saved to: ${REPORT_FILE}" >> "$REPORT_FILE"
    echo "  Logs saved to:   ${LOG_DIR}/" >> "$REPORT_FILE"

    if [ $TOTAL_FAILED -eq 0 ]; then
        echo -e "${GREEN}  ALL TEST SUITES PASSED${NC}"
        echo "  ALL TEST SUITES PASSED" >> "$REPORT_FILE"
        exit 0
    else
        echo -e "${RED}  ${TOTAL_FAILED} TEST SUITE(S) FAILED${NC}"
        echo "  ${TOTAL_FAILED} TEST SUITE(S) FAILED" >> "$REPORT_FILE"
        exit 1
    fi
}

# Main
TARGET="${1:-all}"
NO_FLASH=false

if [[ "$TARGET" == "--no-flash" ]]; then
    NO_FLASH=true
    TARGET="${2:-all}"
fi

if [[ "$TARGET" == "--help" || "$TARGET" == "-h" ]]; then
    echo "Usage: $0 [test_name|all] [--no-flash]"
    echo ""
    echo "Available tests:"
    echo "  test_led         - LED on/off, toggle, patterns"
    echo "  test_sdram       - Fast SDRAM spot-checks (~10s)"
    echo "  test_sdram_full  - Exhaustive SDRAM tests, all 16MB (~3-5min)"
    echo "  test_gpio        - PA0 button input, GPIO output echo"
    echo "  test_uart        - USART1 TX byte, formatted output"
    echo "  test_timers      - TIM2/TIM3 delays, PWM, cancel"
    echo "  test_dma         - DMA2 memory-to-memory transfers"
    echo "  test_lcd         - DSI LCD init, color fills, gradient, stability"
    echo "  test_usb         - USB CDC init, echo (needs host interaction)"
    echo "  test_all         - All non-USB tests in one flash (~60s)"
    echo "  all              - Run fast tests: led, sdram, gpio, uart, timers, dma, lcd"
    echo ""
    echo "Options:"
    echo "  --no-flash  Skip flashing (re-run already-flashed firmware)"
    echo ""
    echo "Results are saved to test-results/"
    exit 0
fi

if [[ "$TARGET" == "all" ]]; then
    run_all "$NO_FLASH"
else
    # Single test
    mkdir -p "$LOG_DIR"
    {
        echo "STM32F469I-DISCO BSP Test Report"
        echo "Date: $(date)"
        echo ""
    } > "$REPORT_FILE"

    case "$TARGET" in
        test_led|test_sdram|test_lcd|test_gpio|test_sdram_full|test_uart|test_timers|test_dma|test_usb|test_all)
            timeout=$TIMEOUT_DEFAULT
            [[ "$TARGET" == "test_sdram" ]] && timeout=60
            [[ "$TARGET" == "test_sdram_full" ]] && timeout=600
            [[ "$TARGET" == "test_lcd" ]] && timeout=120
            [[ "$TARGET" == "test_gpio" ]] && timeout=30
            [[ "$TARGET" == "test_uart" ]] && timeout=30
            [[ "$TARGET" == "test_timers" ]] && timeout=30
            [[ "$TARGET" == "test_dma" ]] && timeout=30
            [[ "$TARGET" == "test_usb" ]] && timeout=30
            [[ "$TARGET" == "test_all" ]] && timeout=120
            flash_and_run "$TARGET" "$timeout"
            ;;
        *)
            echo -e "${RED}Unknown test: ${TARGET}${NC}"
            echo "Available: test_led, test_sdram, test_sdram_full, test_gpio, test_uart, test_timers, test_dma, test_usb, test_lcd, test_all, all"
            exit 1
            ;;
    esac
fi
