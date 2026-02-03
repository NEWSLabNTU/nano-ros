#!/bin/bash
# Setup bridge network for QEMU bare-metal instances
#
# This script creates a Linux bridge with multiple TAP interfaces,
# allowing multiple QEMU mps2-an385 instances to communicate via zenoh.
#
# Network topology:
#   QEMU talker (192.0.3.10/tap-qemu0) --+
#                                        |-- Bridge (qemu-br, 192.0.3.1) -- Host
#   QEMU listener (192.0.3.11/tap-qemu1) +
#
# IP Allocation:
#   192.0.3.1   - Host (bridge interface, zenohd)
#   192.0.3.10  - QEMU node 0 (talker)
#   192.0.3.11  - QEMU node 1 (listener)
#   192.0.3.12+ - Additional QEMU nodes
#
# Usage:
#   sudo ./scripts/qemu/setup-network.sh [OPTIONS] [USERNAME]
#
# Options:
#   --down      Tear down the network
#   --status    Show current status
#   -n N        Create N TAP interfaces (default: 2)
#
# Arguments:
#   USERNAME - User who will run QEMU (default: user who invoked sudo)
#
# After running this script:
#   1. Start a zenoh router on the host:
#      zenohd --listen tcp/0.0.0.0:7447
#
#   2. Run QEMU with networking:
#      ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary your-app.elf
#
# To tear down:
#   sudo ./scripts/qemu/setup-network.sh --down

set -e

BRIDGE_NAME="qemu-br"
TAP_PREFIX="tap-qemu"
HOST_IP="192.0.3.1"
NETMASK="24"
NUM_TAPS=2

# Parse arguments
TEARDOWN=false
STATUS=false
TAP_USER=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --down)
            TEARDOWN=true
            shift
            ;;
        --status)
            STATUS=true
            shift
            ;;
        -n)
            NUM_TAPS="$2"
            shift 2
            ;;
        *)
            TAP_USER="$1"
            shift
            ;;
    esac
done

if [ "$EUID" -ne 0 ] && [ "$STATUS" = false ]; then
    echo "Please run as root: sudo $0"
    exit 1
fi

show_status() {
    echo "QEMU Network Status"
    echo "==================="
    echo ""

    if ip link show "$BRIDGE_NAME" &>/dev/null; then
        echo "Bridge: $BRIDGE_NAME"
        ip addr show "$BRIDGE_NAME" | grep -E "inet |state"
        echo ""

        echo "TAP interfaces:"
        for iface in $(ip link show master "$BRIDGE_NAME" 2>/dev/null | grep -oP "(?<=: )${TAP_PREFIX}\d+(?=[@:])"); do
            echo "  - $iface"
            if [ -f "/sys/class/net/$iface/owner" ]; then
                owner_uid=$(cat "/sys/class/net/$iface/owner")
                owner_name=$(getent passwd "$owner_uid" 2>/dev/null | cut -d: -f1 || echo "UID $owner_uid")
                echo "    Owner: $owner_name"
            fi
        done
        echo ""

        echo "To run QEMU with networking:"
        echo "  ./scripts/qemu/launch-mps2-an385.sh --tap ${TAP_PREFIX}0 --binary your-app.elf"
    else
        echo "Bridge $BRIDGE_NAME does not exist."
        echo ""
        echo "To set up: sudo ./scripts/qemu/setup-network.sh"
    fi
}

teardown() {
    echo "Tearing down QEMU network interfaces..."

    # Find and remove all TAP interfaces from bridge
    for i in $(seq 0 9); do
        tap="${TAP_PREFIX}${i}"
        if ip link show "$tap" &>/dev/null; then
            ip link set "$tap" nomaster 2>/dev/null || true
            ip link set "$tap" down 2>/dev/null || true
            ip tuntap del dev "$tap" mode tap 2>/dev/null || true
            echo "  Removed $tap"
        fi
    done

    # Delete bridge
    if ip link show "$BRIDGE_NAME" &>/dev/null; then
        ip link set "$BRIDGE_NAME" down 2>/dev/null || true
        ip link delete "$BRIDGE_NAME" type bridge 2>/dev/null || true
        echo "  Removed $BRIDGE_NAME"
    fi

    echo "Done."
}

if [ "$STATUS" = true ]; then
    show_status
    exit 0
fi

if [ "$TEARDOWN" = true ]; then
    teardown
    exit 0
fi

# Determine the user who will run QEMU
if [ -z "$TAP_USER" ]; then
    if [ -n "$SUDO_USER" ]; then
        TAP_USER="$SUDO_USER"
    else
        TAP_USER=$(logname 2>/dev/null || echo "")
        if [ -z "$TAP_USER" ]; then
            echo "Error: Could not determine user. Please specify: sudo $0 USERNAME"
            exit 1
        fi
    fi
fi

echo "Setting up bridge network for QEMU bare-metal..."
echo "  TAP owner: $TAP_USER"
echo "  Number of TAP interfaces: $NUM_TAPS"

# Clean up any existing setup
teardown 2>/dev/null || true

# Create bridge
echo ""
echo "Creating bridge $BRIDGE_NAME..."
ip link add name "$BRIDGE_NAME" type bridge
ip addr add "$HOST_IP/$NETMASK" dev "$BRIDGE_NAME"
ip link set "$BRIDGE_NAME" up

# Create TAP interfaces
echo ""
echo "Creating TAP interfaces..."
for i in $(seq 0 $((NUM_TAPS - 1))); do
    tap="${TAP_PREFIX}${i}"
    guest_ip="192.0.3.$((10 + i))"

    echo "  Creating $tap (QEMU guest IP: $guest_ip)..."
    ip tuntap add dev "$tap" mode tap user "$TAP_USER"
    ip link set "$tap" master "$BRIDGE_NAME"
    ip link set "$tap" up
done

# Enable IP forwarding
echo 1 > /proc/sys/net/ipv4/ip_forward

echo ""
echo "========================================"
echo "QEMU bridge network ready!"
echo "========================================"
echo ""
echo "Network configuration:"
echo "  Bridge: $BRIDGE_NAME"
echo "  Host IP: $HOST_IP/$NETMASK"
echo ""
echo "TAP interfaces:"
for i in $(seq 0 $((NUM_TAPS - 1))); do
    tap="${TAP_PREFIX}${i}"
    guest_ip="192.0.3.$((10 + i))"
    echo "  - $tap: QEMU guest IP $guest_ip"
done
echo ""
echo "Owner: $TAP_USER (can run QEMU without sudo)"
echo ""
echo "Next steps:"
echo "  1. Start zenoh router:"
echo "     zenohd --listen tcp/0.0.0.0:7447"
echo ""
echo "  2. Run QEMU with networking:"
echo "     ./scripts/qemu/launch-mps2-an385.sh --tap ${TAP_PREFIX}0 --ip 192.0.2.10 --binary your-app.elf"
echo ""
echo "To verify:"
echo "  ip link show master $BRIDGE_NAME"
echo ""
echo "To tear down:"
echo "  sudo ./scripts/qemu/setup-network.sh --down"
