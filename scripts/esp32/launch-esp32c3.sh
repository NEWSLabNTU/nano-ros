#!/bin/bash
# Launch QEMU ESP32-C3 (RISC-V with OpenCores Ethernet MAC)
#
# This script launches a QEMU ESP32-C3 instance with the open_eth NIC,
# optionally configured with TAP networking for bare-metal network testing.
#
# Usage:
#   ./scripts/esp32/launch-esp32c3.sh [OPTIONS] --binary <bin-file>
#
# Options:
#   --binary FILE     Flash image to run (required, created by espflash save-image)
#   --tap IFACE       TAP interface for networking (e.g., tap-qemu0)
#   --mac MAC         MAC address (default: 02:00:00:00:00:XX based on TAP)
#   --no-network      Disable networking (default if --tap not specified)
#   --gdb             Enable GDB server on port 1234
#   --debug           Print QEMU command without executing
#   -h, --help        Show this help
#
# Examples:
#   # Run without networking (UART output only)
#   ./scripts/esp32/launch-esp32c3.sh --binary build/esp32-qemu/esp32-qemu-talker.bin
#
#   # Run with TAP networking
#   ./scripts/esp32/launch-esp32c3.sh --tap tap-qemu0 --binary build/esp32-qemu/talker.bin
#
#   # Debug with GDB
#   ./scripts/esp32/launch-esp32c3.sh --gdb --binary build/esp32-qemu/talker.bin
#   # Then in another terminal: riscv32-unknown-elf-gdb -ex "target remote :1234" app.elf
#
# Prerequisites:
#   - qemu-system-riscv32 with Espressif ESP32-C3 machine support
#   - For networking: TAP interface setup

set -e

# Default values
BINARY=""
TAP_IFACE=""
MAC_ADDR=""
ENABLE_GDB=false
DEBUG_MODE=false
ENABLE_NETWORK=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --binary)
            BINARY="$2"
            shift 2
            ;;
        --tap)
            TAP_IFACE="$2"
            ENABLE_NETWORK=true
            shift 2
            ;;
        --mac)
            MAC_ADDR="$2"
            shift 2
            ;;
        --no-network)
            ENABLE_NETWORK=false
            shift
            ;;
        --gdb)
            ENABLE_GDB=true
            shift
            ;;
        --debug)
            DEBUG_MODE=true
            shift
            ;;
        -h|--help)
            head -40 "$0" | tail -n +2 | sed 's/^# \?//'
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use --help for usage"
            exit 1
            ;;
    esac
done

# Validate binary
if [ -z "$BINARY" ]; then
    echo "Error: --binary is required"
    echo "Use --help for usage"
    exit 1
fi

if [ ! -f "$BINARY" ]; then
    echo "Error: Binary not found: $BINARY"
    exit 1
fi

# Check QEMU is installed
if ! command -v qemu-system-riscv32 &>/dev/null; then
    echo "Error: qemu-system-riscv32 not found"
    echo "Install Espressif's QEMU fork with ESP32-C3 support"
    exit 1
fi

# Build QEMU command
QEMU_CMD=(
    qemu-system-riscv32
    -M esp32c3
    -icount 3
    -nographic
    -drive "file=$BINARY,if=mtd,format=raw"
)

# Add networking if enabled
if [ "$ENABLE_NETWORK" = true ]; then
    if [ -z "$TAP_IFACE" ]; then
        echo "Error: --tap is required when networking is enabled"
        exit 1
    fi

    # Check TAP interface exists
    if ! ip link show "$TAP_IFACE" &>/dev/null; then
        echo "Error: TAP interface $TAP_IFACE does not exist"
        echo "Set up TAP networking first"
        exit 1
    fi

    # Generate MAC address from TAP interface number if not specified
    if [ -z "$MAC_ADDR" ]; then
        tap_num="${TAP_IFACE##*[!0-9]}"
        tap_num="${tap_num:-0}"
        MAC_ADDR=$(printf "02:00:00:00:00:%02x" "$tap_num")
    fi

    # Add network device (open_eth model for OpenCores Ethernet MAC)
    QEMU_CMD+=(
        -nic "tap,model=open_eth,ifname=$TAP_IFACE,script=no,downscript=no,mac=$MAC_ADDR"
    )

    echo "Network configuration:"
    echo "  TAP interface: $TAP_IFACE"
    echo "  MAC address: $MAC_ADDR"
    echo ""
else
    # No networking
    QEMU_CMD+=(-nic none)
fi

# Add GDB if enabled
if [ "$ENABLE_GDB" = true ]; then
    QEMU_CMD+=(-s -S)
    echo "GDB server enabled on port 1234"
    echo ""
fi

# Debug mode - just print command
if [ "$DEBUG_MODE" = true ]; then
    echo "QEMU command:"
    echo "  ${QEMU_CMD[*]}"
    exit 0
fi

# Run QEMU
echo "Launching QEMU ESP32-C3..."
echo "Binary: $BINARY"
echo ""
exec "${QEMU_CMD[@]}"
