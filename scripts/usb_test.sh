#!/bin/bash
# USB Standalone Test - Flash and Run
#
# This script tests USB CDC without probe-rs interference.
# It flashes the standalone firmware, disconnects from SWD,
# then runs the host-side test via USB serial.
#
# Usage: ./scripts/usb_test.sh [duration_seconds] [device]
#
# Prerequisites:
#   - st-flash (apt install stlink-tools)
#   - arm-none-eabi-objcopy
#   - python3 with pyserial (pip install pyserial)
#
# Exit codes:
#   0 - All tests passed
#   1 - Build or test failure
#   2 - Device not found

set -euo pipefail

DURATION=${1:-30}
DEVICE=${2:-/dev/ttyACM0}
CHIP="STM32F469NIHx"
EXAMPLE="test_usb_standalone"
ELF_PATH="target/thumbv7em-none-eabihf/release/examples/${EXAMPLE}"
BIN_PATH="/tmp/${EXAMPLE}.bin"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

echo -e "${CYAN}==========================================${NC}"
echo -e "${CYAN}  USB Standalone Test${NC}"
echo -e "${CYAN}==========================================${NC}"
echo ""

# Step 1: Build
echo -e "${YELLOW}[1/4] Building ${EXAMPLE}...${NC}"
cargo build --release --example "$EXAMPLE" 2>&1
echo -e "${GREEN}  Build OK${NC}"

# Step 2: Convert to binary
echo -e "${YELLOW}[2/4] Converting to binary...${NC}"
arm-none-eabi-objcopy -O binary "$ELF_PATH" "$BIN_PATH"
echo -e "${GREEN}  Binary: ${BIN_PATH} ($(wc -c < "$BIN_PATH") bytes)${NC}"

# Step 3: Flash with st-flash
echo -e "${YELLOW}[3/4] Flashing with st-flash...${NC}"
if ! st-flash --connect-under-reset write "$BIN_PATH" 0x08000000 2>&1; then
    echo -e "${RED}  Flash failed${NC}"
    echo -e "${RED}  Try: sudo st-flash --connect-under-reset reset${NC}"
    exit 1
fi
echo -e "${GREEN}  Flash OK${NC}"

# Step 4: Reset and run host test
echo -e "${YELLOW}[4/4] Resetting device...${NC}"
st-flash --connect-under-reset reset 2>&1 || true
echo ""
echo -e "${CYAN}  IMPORTANT: Disconnect ST-Link if possible, or at least${NC}"
echo -e "${CYAN}  do not attach probe-rs while USB test is running.${NC}"
echo ""
echo -e "${YELLOW}Waiting 3s for USB enumeration...${NC}"
sleep 3

echo -e "${YELLOW}Running host-side USB test (${DURATION}s)...${NC}"
echo ""

if python3 tests/host/test_usb_host.py --device "$DEVICE" --duration "$DURATION"; then
    echo ""
    echo -e "${GREEN}=========================================="
    echo -e "  USB STANDALONE TEST: PASSED"
    echo -e "==========================================${NC}"
    exit 0
else
    exit_code=$?
    echo ""
    echo -e "${RED}=========================================="
    echo -e "  USB STANDALONE TEST: FAILED (exit ${exit_code})"
    echo -e "==========================================${NC}"
    exit 1
fi
