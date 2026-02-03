#!/bin/bash
# Test: Zephyr C Examples (native_sim)
#
# This test verifies that the nano-ros C API examples (zephyr-c-talker)
# running on Zephyr RTOS (native_sim) can successfully publish messages
# via zenoh.
#
# Prerequisites:
#   - Zephyr workspace set up (./scripts/zephyr/setup.sh)
#   - TAP network configured (sudo ./scripts/zephyr/setup-network.sh)
#   - zenohd installed
#
# Usage:
#   ./tests/zephyr/run-c.sh
#   ./tests/zephyr/run-c.sh --verbose
#   ./tests/zephyr/run-c.sh --skip-build
#
# Note: This test requires the Zephyr workspace to be initialized.
# Run ./scripts/zephyr/setup.sh first if not done.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# =============================================================================
# Utilities (self-contained)
# =============================================================================

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Logging functions
log_info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[PASS]${NC} $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error()   { echo -e "${RED}[FAIL]${NC} $*"; }
log_header()  { echo -e "\n${CYAN}=== $* ===${NC}"; }

# Temp directory for this test run
TEST_TMPDIR="$(mktemp -d /tmp/nano-ros-zephyr-test.XXXXXX)"

# Get a temp file path
tmpfile() {
    echo "$TEST_TMPDIR/$1"
}

# PIDs for cleanup
declare -a CLEANUP_PIDS=()

# Register a PID for cleanup
register_pid() {
    CLEANUP_PIDS+=("$1")
}

# Cleanup function - kills all registered processes
cleanup() {
    log_info "Cleaning up..."

    # Kill registered PIDs
    for pid in "${CLEANUP_PIDS[@]}"; do
        if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
            kill "$pid" 2>/dev/null || true
        fi
    done

    # Also kill by name as fallback
    pkill -x zenohd 2>/dev/null || true

    # Clean up temp directory
    if [ -n "$TEST_TMPDIR" ] && [ -d "$TEST_TMPDIR" ]; then
        rm -rf "$TEST_TMPDIR"
    fi

    CLEANUP_PIDS=()
}

# Setup trap for cleanup
setup_cleanup() {
    trap cleanup EXIT INT TERM
}

# =============================================================================
# Script Configuration
# =============================================================================

# Parse arguments
VERBOSE=false
SKIP_BUILD=false
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose|-v) VERBOSE=true; shift ;;
        --skip-build) SKIP_BUILD=true; shift ;;
        *) shift ;;
    esac
done

# Configuration - workspace location (via symlink or env var)
# Priority: 1. ZEPHYR_NANO_ROS env var, 2. zephyr-workspace symlink, 3. sibling workspace
if [ -n "${ZEPHYR_NANO_ROS:-}" ]; then
    ZEPHYR_WORKSPACE="$ZEPHYR_NANO_ROS"
elif [ -L "$PROJECT_ROOT/zephyr-workspace" ]; then
    ZEPHYR_WORKSPACE="$(readlink -f "$PROJECT_ROOT/zephyr-workspace")"
else
    NANO_ROS_NAME="$(basename "$PROJECT_ROOT")"
    ZEPHYR_WORKSPACE="$(dirname "$PROJECT_ROOT")/${NANO_ROS_NAME}-workspace"
fi
# TAP interface: Use bridge (zeth-br) which has the host IP
TAP_INTERFACE="zeth-br"
HOST_IP="192.0.2.2"
ZEPHYR_IP="192.0.2.1"
TEST_TIMEOUT=15

setup_cleanup

log_header "Zephyr C Examples Test (native_sim)"

# =============================================================================
# Network Device Status Check
# =============================================================================

check_network_device() {
    log_header "Checking Network Device Status"

    local status=0

    # Check if TAP interface exists
    if ! ip link show "$TAP_INTERFACE" &>/dev/null; then
        log_error "TAP interface '$TAP_INTERFACE' does not exist"
        log_info "Run: sudo ./scripts/zephyr/setup-network.sh"
        return 1
    fi
    log_success "TAP interface '$TAP_INTERFACE' exists"

    # Check interface state (UP flag)
    local flags
    flags=$(ip link show "$TAP_INTERFACE" | head -1)
    if echo "$flags" | grep -q "UP"; then
        log_success "Interface is UP"
    else
        log_error "Interface is DOWN"
        log_info "Run: sudo ip link set $TAP_INTERFACE up"
        status=1
    fi

    # Check IP address configuration
    local ip_addr
    ip_addr=$(ip -4 addr show "$TAP_INTERFACE" 2>/dev/null | grep -oP 'inet \K[\d.]+' | head -1)
    if [ "$ip_addr" = "$HOST_IP" ]; then
        log_success "IP address configured: $ip_addr"
    elif [ -n "$ip_addr" ]; then
        log_warn "Unexpected IP address: $ip_addr (expected $HOST_IP)"
    else
        log_error "No IP address configured on $TAP_INTERFACE"
        log_info "Run: sudo ip addr add $HOST_IP/24 dev $TAP_INTERFACE"
        status=1
    fi

    # Check interface ownership (should be owned by current user)
    local owner_file="/sys/class/net/$TAP_INTERFACE/owner"
    if [ -f "$owner_file" ]; then
        local owner_uid
        owner_uid=$(cat "$owner_file")
        local my_uid
        my_uid=$(id -u)
        if [ "$owner_uid" = "$my_uid" ]; then
            log_success "Interface owned by current user (UID: $my_uid)"
        else
            log_warn "Interface owned by UID $owner_uid (current: $my_uid)"
        fi
    fi

    # Check carrier state
    local carrier_state
    carrier_state=$(cat "/sys/class/net/$TAP_INTERFACE/carrier" 2>/dev/null || echo "0")
    if [ "$carrier_state" = "1" ]; then
        log_success "Carrier detected (link up)"
    else
        log_info "No carrier (expected when Zephyr not running)"
    fi

    # Show full interface info in verbose mode
    if [ "$VERBOSE" = true ]; then
        echo ""
        echo "=== Interface Details ==="
        ip addr show "$TAP_INTERFACE"
        echo ""
    fi

    return $status
}

# =============================================================================
# Prerequisites Check
# =============================================================================

check_zephyr_prerequisites() {
    log_header "Checking Zephyr Prerequisites"

    local missing=0

    # Check Zephyr workspace
    if [ -d "$ZEPHYR_WORKSPACE" ]; then
        log_success "Zephyr workspace found: $ZEPHYR_WORKSPACE"

        # Check for zephyr subdirectory
        if [ -d "$ZEPHYR_WORKSPACE/zephyr" ]; then
            log_success "Zephyr SDK found"
        else
            log_error "Zephyr SDK not found in workspace"
            missing=1
        fi
    else
        log_error "Zephyr workspace not found at $ZEPHYR_WORKSPACE"
        log_info "Run: ./scripts/zephyr/setup.sh"
        missing=1
    fi

    # Check west
    if command -v west &>/dev/null; then
        log_success "west found: $(which west)"
    else
        log_error "west not found"
        missing=1
    fi

    # Check zenohd
    if command -v zenohd &>/dev/null; then
        log_success "zenohd found: $(which zenohd)"
    else
        log_error "zenohd not found"
        missing=1
    fi

    # Check for existing build
    if [ -f "$ZEPHYR_WORKSPACE/build-c-talker/zephyr/zephyr.exe" ]; then
        log_success "Zephyr C talker executable found"
    else
        log_info "Zephyr C talker executable not found (will build)"
    fi

    return $missing
}

# =============================================================================
# Build Zephyr Examples
# =============================================================================

build_zephyr_examples() {
    log_header "Building Zephyr Examples"

    cd "$ZEPHYR_WORKSPACE"

    # Source environment
    if [ -f ".venv/bin/activate" ]; then
        # shellcheck source=/dev/null
        source .venv/bin/activate
    fi
    if [ -f "zephyr/zephyr-env.sh" ]; then
        # shellcheck source=/dev/null
        source zephyr/zephyr-env.sh
    fi
    export ZEPHYR_BASE="$ZEPHYR_WORKSPACE/zephyr"

    # Determine example path based on workspace type
    # In-tree workspace: examples at ../examples/
    # External workspace: examples at nano-ros/examples/
    local example_path
    if [ -d "nano-ros/examples" ]; then
        example_path="nano-ros/examples/zephyr/c-talker"
    else
        example_path="$PROJECT_ROOT/examples/zephyr/c-talker"
    fi

    # Build C talker for native_sim/native/64
    log_info "Building zephyr-c-talker for native_sim/native/64..."
    if west build -b native_sim/native/64 "$example_path" -d build-c-talker -p auto 2>&1 | tee "$(tmpfile zephyr_build.txt)" | tail -10; then
        log_success "Talker build complete"
    else
        log_error "Talker build failed"
        [ "$VERBOSE" = true ] && cat "$(tmpfile zephyr_build.txt)"
        return 1
    fi

    return 0
}

# =============================================================================
# Test: Zephyr Talker -> Native Subscriber
# =============================================================================

test_zephyr_to_native() {
    log_header "Test: Zephyr C Talker"

    # Start zenoh router
    log_info "Starting zenoh router..."
    pkill -x zenohd 2>/dev/null || true
    sleep 1
    zenohd --listen tcp/0.0.0.0:7447 > "$(tmpfile zephyr_zenohd.txt)" 2>&1 &
    local zenohd_pid=$!
    register_pid $zenohd_pid
    sleep 2

    if ! kill -0 $zenohd_pid 2>/dev/null; then
        log_error "Failed to start zenohd"
        cat "$(tmpfile zephyr_zenohd.txt)"
        return 1
    fi
    log_success "zenohd started (PID: $zenohd_pid)"

    # Start Zephyr C talker
    log_info "Starting Zephyr C talker..."
    cd "$ZEPHYR_WORKSPACE"
    timeout "$TEST_TIMEOUT" ./build-c-talker/zephyr/zephyr.exe > "$(tmpfile zephyr_talker.txt)" 2>&1 &
    local zephyr_pid=$!
    register_pid $zephyr_pid

    # Wait for messages to be published
    log_info "Waiting for Zephyr to publish messages (timeout: ${TEST_TIMEOUT}s)..."

    local elapsed=0
    while [ $elapsed -lt $TEST_TIMEOUT ]; do
        # Check if talker has published at least 3 messages
        local count
        count=$(grep -c "Published:" "$(tmpfile zephyr_talker.txt)" 2>/dev/null | head -1 || echo "0")
        count="${count:-0}"
        if [ "$count" -ge 3 ] 2>/dev/null; then
            break
        fi
        # Check for errors
        if grep -q "Failed to" "$(tmpfile zephyr_talker.txt)" 2>/dev/null; then
            break
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done

    # Check results
    local pub_count
    pub_count=$(grep -c "Published:" "$(tmpfile zephyr_talker.txt)" 2>/dev/null || echo 0)

    if [ "$pub_count" -ge 3 ]; then
        log_success "Zephyr C talker published $pub_count messages successfully!"

        if [ "$VERBOSE" = true ]; then
            echo ""
            echo "=== Zephyr Talker Output ==="
            head -30 "$(tmpfile zephyr_talker.txt)"
        fi
        return 0
    else
        log_error "Zephyr C talker failed to publish messages"
        echo ""
        echo "=== Zephyr Output ==="
        cat "$(tmpfile zephyr_talker.txt)" 2>/dev/null | head -40
        echo ""
        echo "=== zenohd Output ==="
        cat "$(tmpfile zephyr_zenohd.txt)" 2>/dev/null | tail -10
        return 1
    fi
}

# =============================================================================
# Main
# =============================================================================

RESULT=0

# Check network device first
if ! check_network_device; then
    log_error "Network device not properly configured"
    log_info ""
    log_info "To set up TAP networking:"
    log_info "  sudo ./scripts/zephyr/setup-network.sh"
    log_info ""
    log_info "To check network status manually:"
    log_info "  ip addr show $TAP_INTERFACE"
    exit 1
fi

# Check prerequisites
if ! check_zephyr_prerequisites; then
    log_error "Prerequisites not met"
    log_info ""
    log_info "To set up the Zephyr workspace:"
    log_info "  ./scripts/zephyr/setup.sh"
    exit 1
fi

# Build examples
if [ "$SKIP_BUILD" = false ]; then
    if ! build_zephyr_examples; then
        log_error "Build failed"
        exit 1
    fi
fi

# Run test
test_zephyr_to_native || RESULT=1

# Summary
log_header "Test Summary"
if [ $RESULT -eq 0 ]; then
    log_success "Zephyr C examples test passed!"
else
    log_error "Zephyr C examples test failed"
    log_info ""
    log_info "Troubleshooting:"
    log_info "  1. Check TAP interface: ip addr show $TAP_INTERFACE"
    log_info "  2. Check zenohd is accessible: zenohd --listen tcp/0.0.0.0:7447"
    log_info "  3. Check Zephyr can reach host: ping $HOST_IP (from Zephyr)"
    log_info "  4. Run with --verbose for detailed output"
fi

exit $RESULT
