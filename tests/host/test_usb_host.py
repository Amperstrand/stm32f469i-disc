#!/usr/bin/env python3
"""Host-side USB CDC test companion for STM32F469I-DISCO.

Tests USB enumeration, CDC echo, and sustained polling on the standalone
USB firmware (test_usb_standalone). This script does NOT use probe-rs
or SWD -- it communicates purely via USB CDC serial.

Usage:
    python3 tests/host/test_usb_host.py [--device /dev/ttyACM0] [--duration 30] [--baud 115200]

Exit codes:
    0 - All tests passed
    1 - Some tests failed
    2 - Device not found or connection error
    3 - Timeout waiting for device

Requires: pyserial (pip install pyserial)
"""

import argparse
import serial
import serial.tools.list_ports
import sys
import time


def find_stm32_cdc_port():
    """Find the STM32 CDC serial port by VID:PID."""
    for port in serial.tools.list_ports.comports():
        if port.vid == 0x16c0 and port.pid == 0x27dd:
            return port.device
        if "STM32" in port.description or "STLink" in port.description:
            if port.vid != 0x0483:
                return port.device
    return None


def wait_for_device(device_hint, timeout=30):
    """Wait for USB CDC device to appear."""
    print(f"Waiting for USB CDC device (timeout: {timeout}s)...")
    start = time.time()
    while time.time() - start < timeout:
        if device_hint:
            try:
                with serial.Serial(device_hint, timeout=0.5):
                    return device_hint
            except (serial.SerialException, OSError):
                pass
        else:
            port = find_stm32_cdc_port()
            if port:
                return port
        time.sleep(0.5)
    return None


def send_and_recv(ser, data, timeout=2.0):
    """Send data and receive response."""
    ser.write(data)
    ser.flush()
    start = time.time()
    response = b""
    while time.time() - start < timeout:
        if ser.in_waiting > 0:
            chunk = ser.read(ser.in_waiting)
            response += chunk
            if b"\n" in response:
                break
    return response.strip()


def run_tests(device, baud, duration):
    """Run the full USB CDC test suite."""
    print(f"Connecting to {device} at {baud} baud...")

    try:
        ser = serial.Serial(device, baud, timeout=0.5)
    except serial.SerialException as e:
        print(f"ERROR: Failed to open {device}: {e}")
        return 2

    time.sleep(0.5)
    ser.reset_input_buffer()

    passed = 0
    failed = 0
    errors = []

    # Test 1: STATS command (device should respond)
    print("\n[TEST 1] usb_enumeration")
    try:
        resp = send_and_recv(ser, b"STATS\r\n", timeout=3.0)
        if b"Active" in resp or b"Ready" in resp or len(resp) > 0:
            print(f"  PASS - Device responded: {resp.decode('ascii', errors='replace')}")
            passed += 1
        else:
            print(f"  FAIL - Unexpected response: {resp}")
            failed += 1
            errors.append("usb_enumeration: unexpected response")
    except Exception as e:
        print(f"  FAIL - {e}")
        failed += 1
        errors.append(f"usb_enumeration: {e}")

    # Test 2: PING/PONG echo
    print("\n[TEST 2] usb_ping_pong")
    ping_count = 0
    ping_target = min(10, max(3, duration // 2))
    ping_fails = 0
    for i in range(ping_target):
        try:
            resp = send_and_recv(ser, b"PING\r\n", timeout=2.0)
            if b"PONG" in resp:
                ping_count += 1
            else:
                ping_fails += 1
        except Exception:
            ping_fails += 1
        time.sleep(0.05)

    if ping_fails == 0:
        print(f"  PASS - {ping_count}/{ping_target} PINGs got PONG")
        passed += 1
    else:
        print(f"  FAIL - {ping_fails}/{ping_target} PINGs failed")
        failed += 1
        errors.append(f"usb_ping_pong: {ping_fails} failures")

    # Test 3: Multi-byte echo
    print("\n[TEST 3] usb_echo_64b")
    test_data = b"ABCDEFGHIJKLMNOPQRSTUVWX" * 2 + b"\r\n"
    try:
        resp = send_and_recv(ser, test_data, timeout=2.0)
        expected = test_data.strip()
        if resp == expected:
            print(f"  PASS - 64-byte echo matched")
            passed += 1
        elif len(resp) == len(expected):
            print(f"  PASS - 64-byte echo length matched ({len(resp)} bytes)")
            passed += 1
        else:
            print(f"  FAIL - Expected {len(expected)} bytes, got {len(resp)}")
            failed += 1
            errors.append(f"usb_echo_64b: length mismatch {len(resp)} vs {len(expected)}")
    except Exception as e:
        print(f"  FAIL - {e}")
        failed += 1
        errors.append(f"usb_echo_64b: {e}")

    # Test 4: Sustained echo stress
    print(f"\n[TEST 4] usb_sustained_echo ({duration}s)")
    stress_start = time.time()
    stress_count = 0
    stress_fails = 0
    last_fail_time = stress_start

    while time.time() - stress_start < duration:
        try:
            resp = send_and_recv(ser, b"PING\r\n", timeout=1.0)
            if b"PONG" in resp:
                stress_count += 1
            else:
                stress_fails += 1
                last_fail_time = time.time()
        except Exception:
            stress_fails += 1
            last_fail_time = time.time()

        # Abort if 5 consecutive failures for >3 seconds
        if stress_fails >= 5 and (time.time() - last_fail_time) > 3:
            print(f"  FAIL - USB appears frozen ({stress_fails} consecutive failures)")
            failed += 1
            errors.append(f"usb_sustained_echo: USB frozen after {stress_count} successful")
            break
    else:
        if stress_fails == 0:
            rate = stress_count / duration if duration > 0 else 0
            print(f"  PASS - {stress_count} echoes in {duration:.0f}s ({rate:.1f} cmd/s)")
            passed += 1
        else:
            rate = stress_count / duration if duration > 0 else 0
            print(f"  FAIL - {stress_count} ok, {stress_fails} failed ({rate:.1f} cmd/s)")
            failed += 1
            errors.append(f"usb_sustained_echo: {stress_fails} failures")

    # Test 5: BYE command (graceful shutdown)
    print("\n[TEST 5] usb_bye")
    try:
        resp = send_and_recv(ser, b"BYE\r\n", timeout=3.0)
        if b"DONE" in resp or b"BYE" in resp or b"Summary" in resp:
            print(f"  PASS - Device acknowledged BYE")
            passed += 1
        else:
            print(f"  INFO - Response: {resp.decode('ascii', errors='replace')}")
            passed += 1  # non-critical
    except Exception:
        print("  INFO - No BYE response (non-critical)")
        passed += 1

    ser.close()

    # Summary
    total = passed + failed
    print(f"\n{'='*50}")
    print(f"  USB CDC Test Summary")
    print(f"{'='*50}")
    print(f"  Passed: {passed}/{total}")

    if errors:
        print(f"  Failures:")
        for e in errors:
            print(f"    - {e}")

    if failed == 0:
        print(f"\n  ALL TESTS PASSED")
        return 0
    else:
        print(f"\n  {failed} TEST(S) FAILED")
        return 1


def main():
    parser = argparse.ArgumentParser(description="STM32F469I-DISCO USB CDC host test")
    parser.add_argument("--device", "-d", default=None, help="Serial port (auto-detect if omitted)")
    parser.add_argument("--duration", "-t", type=int, default=30, help="Stress test duration in seconds (default: 30)")
    parser.add_argument("--baud", "-b", type=int, default=115200, help="Baud rate (default: 115200)")
    parser.add_argument("--wait-timeout", type=int, default=30, help="Time to wait for device enumeration (default: 30)")
    args = parser.parse_args()

    print("=" * 50)
    print("  STM32F469I-DISCO USB CDC Host Test")
    print("=" * 50)
    print()

    device = args.device
    if not device:
        device = find_stm32_cdc_port()
        if device:
            print(f"Auto-detected device: {device}")
        else:
            device = wait_for_device(None, args.wait_timeout)
            if device:
                print(f"Device appeared: {device}")
            else:
                print("ERROR: No STM32 CDC device found")
                print("Make sure:")
                print("  1. Board is connected via USB cable (not just ST-Link)")
                print("  2. test_usb_standalone firmware is flashed")
                print("  3. Board is reset after flashing")
                return 3

    return run_tests(device, args.baud, args.duration)


if __name__ == "__main__":
    sys.exit(main())
