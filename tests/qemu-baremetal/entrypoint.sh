#!/bin/bash
# QEMU Container Entrypoint
#
# Sets up internal TAP networking with NAT and runs QEMU.
# Each container has its own isolated network namespace with:
# - Internal bridge (192.168.100.0/24)
# - TAP device for QEMU
# - NAT to reach the Docker network (and zenohd)
#
# Environment variables:
#   ZENOH_ROUTER_IP - IP of zenohd container (default: 172.20.0.2)
#   QEMU_ROLE       - "talker" or "listener"
#   QEMU_MAC        - MAC address for QEMU NIC
#   QEMU_EXAMPLE    - Example type: "rs" (default), "bsp"

set -e

# Configuration
BRIDGE_NAME="br0"
BRIDGE_IP="192.168.100.1"
BRIDGE_MASK="24"
TAP_NAME="tap0"

ZENOH_ROUTER_IP="${ZENOH_ROUTER_IP:-172.20.0.2}"
QEMU_ROLE="${QEMU_ROLE:-talker}"
QEMU_MAC="${QEMU_MAC:-02:00:00:00:00:00}"
QEMU_EXAMPLE="${QEMU_EXAMPLE:-rs}"

# Determine binary path based on example type
if [ "$QEMU_EXAMPLE" = "bsp" ]; then
    EXAMPLE_DIR="bsp-${QEMU_ROLE}"
    BINARY_NAME="qemu-bsp-${QEMU_ROLE}"
else
    EXAMPLE_DIR="rs-${QEMU_ROLE}"
    BINARY_NAME="qemu-rs-${QEMU_ROLE}"
fi

BINARY="/work/examples/qemu/${EXAMPLE_DIR}/target/thumbv7m-none-eabi/release/${BINARY_NAME}"

echo "============================================"
echo "  QEMU Container: ${QEMU_ROLE} (${QEMU_EXAMPLE})"
echo "============================================"
echo ""
echo "Configuration:"
echo "  Zenoh router: ${ZENOH_ROUTER_IP}:7447"
echo "  QEMU MAC: ${QEMU_MAC}"
echo "  Example: ${QEMU_EXAMPLE}-${QEMU_ROLE}"
echo "  Binary: ${BINARY}"
echo ""

# Check binary exists, build if necessary
if [ ! -f "$BINARY" ]; then
    echo "Binary not found: $BINARY"
    echo ""
    echo "Building example with Docker network configuration..."

    # Build with docker feature
    cd "/work/examples/qemu/${EXAMPLE_DIR}"
    cargo build --release --features docker

    if [ ! -f "$BINARY" ]; then
        echo "Error: Build failed"
        exit 1
    fi
    echo "Build successful!"
fi

echo "Step 1: Setting up internal network..."

# Create bridge
ip link add name "$BRIDGE_NAME" type bridge
ip addr add "${BRIDGE_IP}/${BRIDGE_MASK}" dev "$BRIDGE_NAME"
ip link set "$BRIDGE_NAME" up

echo "  Created bridge: $BRIDGE_NAME ($BRIDGE_IP/$BRIDGE_MASK)"

# Create TAP interface
ip tuntap add dev "$TAP_NAME" mode tap
ip link set "$TAP_NAME" master "$BRIDGE_NAME"
ip link set "$TAP_NAME" up

echo "  Created TAP: $TAP_NAME"

# Enable proxy ARP so the bridge responds to ARP requests from QEMU
echo 1 > /proc/sys/net/ipv4/conf/all/proxy_arp 2>/dev/null || true
echo 1 > /proc/sys/net/ipv4/conf/$BRIDGE_NAME/proxy_arp 2>/dev/null || true

# IP forwarding is enabled via docker-compose sysctls
# Verify it's enabled
if [ "$(cat /proc/sys/net/ipv4/ip_forward)" != "1" ]; then
    echo "Warning: IP forwarding not enabled"
fi

# Set up NAT (masquerade traffic from QEMU subnet going to Docker network)
# Find the main interface (usually eth0 in Docker)
MAIN_IF=$(ip route | grep default | awk '{print $5}' | head -1)
echo "  Main interface: $MAIN_IF"

# Install iptables if not available (minimal containers)
if ! command -v iptables &>/dev/null; then
    echo "  Installing iptables..."
    apt-get update -qq && apt-get install -y -qq iptables >/dev/null 2>&1
fi

# NAT rules (use --random to avoid port conflicts between containers)
iptables -t nat -A POSTROUTING -s 192.168.100.0/24 -o "$MAIN_IF" -j MASQUERADE --random || echo "  Warning: MASQUERADE rule failed"
iptables -A FORWARD -i "$BRIDGE_NAME" -o "$MAIN_IF" -j ACCEPT || echo "  Warning: FORWARD (out) rule failed"
iptables -A FORWARD -i "$MAIN_IF" -o "$BRIDGE_NAME" -m state --state RELATED,ESTABLISHED -j ACCEPT || echo "  Warning: FORWARD (in) rule failed"

echo "  NAT configured for outgoing traffic"

echo ""
echo "Step 2: Testing connectivity to zenohd..."

# Wait for zenohd to be reachable
MAX_WAIT=30
WAITED=0
while ! nc -z -w1 "$ZENOH_ROUTER_IP" 7447 2>/dev/null; do
    if [ $WAITED -ge $MAX_WAIT ]; then
        echo "  Error: Cannot reach zenohd at ${ZENOH_ROUTER_IP}:7447"
        exit 1
    fi
    sleep 1
    WAITED=$((WAITED + 1))
done
echo "  zenohd reachable at ${ZENOH_ROUTER_IP}:7447"

# Give more time for network setup to stabilize
if [ "$QEMU_ROLE" = "listener" ]; then
    echo "  Listener: waiting 5s for network to stabilize..."
    sleep 5
elif [ "$QEMU_ROLE" = "talker" ]; then
    echo "  Talker: waiting 10s for listener to connect first..."
    sleep 10
fi

echo ""
echo "Step 3: Starting QEMU ${QEMU_ROLE}..."
echo ""

# Run QEMU
# Note: mps2-an385 uses legacy -net syntax for lan9118, not -device
exec qemu-system-arm \
    -cpu cortex-m3 \
    -machine mps2-an385 \
    -nographic \
    -semihosting-config "enable=on,target=native" \
    -net "nic,model=lan9118,macaddr=${QEMU_MAC}" \
    -net "tap,ifname=${TAP_NAME},script=no,downscript=no" \
    -kernel "$BINARY"
