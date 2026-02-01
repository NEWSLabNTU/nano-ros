#!/bin/bash
# Launch QEMU mps2-an385 (Cortex-M3 with LAN9118 Ethernet)
#
# This script launches a QEMU instance with the MPS2-AN385 machine,
# optionally configured with TAP networking for bare-metal network testing.
#
# Usage:
#   ./scripts/qemu/launch-mps2-an385.sh [OPTIONS] --binary <elf-file>
#
# Options:
#   --binary FILE     ELF binary to run (required)
#   --tap IFACE       TAP interface for networking (e.g., tap-qemu0)
#   --ip IP           Guest IP address (default: 192.0.2.10)
#   --mac MAC         MAC address (default: 02:00:00:00:00:XX based on TAP)
#   --no-network      Disable networking (default if --tap not specified)
#   --gdb             Enable GDB server on port 1234
#   --debug           Print QEMU command without executing
#   -h, --help        Show this help
#
# Examples:
#   # Run without networking (semihosting only)
#   ./scripts/qemu/launch-mps2-an385.sh --binary target/thumbv7m-none-eabi/release/app
#
#   # Run with TAP networking
#   ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --ip 192.0.2.10 --binary app.elf
#
#   # Debug with GDB
#   ./scripts/qemu/launch-mps2-an385.sh --gdb --binary app.elf
#   # Then in another terminal: arm-none-eabi-gdb -ex "target remote :1234" app.elf
#
# Prerequisites:
#   - qemu-system-arm installed
#   - For networking: sudo ./scripts/qemu/setup-network.sh

set -e

# Default values
BINARY=""
TAP_IFACE=""
GUEST_IP="192.0.2.10"
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
        --ip)
            GUEST_IP="$2"
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
if ! command -v qemu-system-arm &>/dev/null; then
    echo "Error: qemu-system-arm not found"
    echo "Install with: sudo apt install qemu-system-arm"
    exit 1
fi

# Build QEMU command
QEMU_CMD=(
    qemu-system-arm
    -cpu cortex-m3
    -machine mps2-an385
    -nographic
    -semihosting-config "enable=on,target=native"
    -kernel "$BINARY"
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
        echo "Run: sudo ./scripts/qemu/setup-network.sh"
        exit 1
    fi

    # Generate MAC address from TAP interface number if not specified
    if [ -z "$MAC_ADDR" ]; then
        tap_num="${TAP_IFACE##*[!0-9]}"
        tap_num="${tap_num:-0}"
        MAC_ADDR=$(printf "02:00:00:00:00:%02x" "$tap_num")
    fi

    # Add network device
    # Use -net syntax for broader QEMU version compatibility
    # The mps2-an385 machine has a built-in LAN9118 that connects to -net nic
    QEMU_CMD+=(
        -net "nic,model=lan9118,macaddr=$MAC_ADDR"
        -net "tap,ifname=$TAP_IFACE,script=no,downscript=no"
    )

    echo "Network configuration:"
    echo "  TAP interface: $TAP_IFACE"
    echo "  Guest IP: $GUEST_IP (configure in your application)"
    echo "  MAC address: $MAC_ADDR"
    echo ""
fi

# Add GDB if enabled
if [ "$ENABLE_GDB" = true ]; then
    QEMU_CMD+=(-s -S)
    echo "GDB server enabled on port 1234"
    echo "Connect with: arm-none-eabi-gdb -ex 'target remote :1234' $BINARY"
    echo ""
fi

# Debug mode - just print command
if [ "$DEBUG_MODE" = true ]; then
    echo "QEMU command:"
    echo "  ${QEMU_CMD[*]}"
    exit 0
fi

# Run QEMU
echo "Launching QEMU mps2-an385..."
echo "Binary: $BINARY"
echo ""
exec "${QEMU_CMD[@]}"
