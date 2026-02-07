#!/usr/bin/env bash
# C-based integration tests for nano-ros-c
#
# This script builds and tests the C examples (native-c-talker, native-c-listener).
#
# Usage:
#   ./tests/c-tests.sh             # Build and run all C tests
#   ./tests/c-tests.sh --verbose   # Show detailed output
#   ./tests/c-tests.sh --skip-build # Skip build step (use existing binaries)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Use locally-built zenohd if available, otherwise fall back to system PATH
ZENOHD="$PROJECT_ROOT/build/zenohd/zenohd"
[ -x "$ZENOHD" ] || ZENOHD="zenohd"

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
log_section() { echo -e "\n${CYAN}=== $* ===${NC}"; }

# Temp directory for test outputs
TEST_TMPDIR="$(mktemp -d /tmp/nano-ros-c-test.XXXXXX)"

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

# Cleanup function
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
trap cleanup EXIT INT TERM

usage() {
    cat <<EOF
Usage: $(basename "$0") [OPTIONS]

Build and run C integration tests for nano-ros-c.

OPTIONS:
  -v, --verbose      Show verbose output
  --skip-build       Skip build step (use existing binaries)
  -h, --help         Show this help

EXAMPLES:
  $(basename "$0")                 # Build and run all tests
  $(basename "$0") --verbose       # Verbose output
  $(basename "$0") --skip-build    # Skip build, run tests only

REQUIREMENTS:
  - cmake (for building C examples)
  - zenohd (for routing)
  - Rust toolchain (for building nano-ros-c)
EOF
}

VERBOSE=false
SKIP_BUILD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -v|--verbose)
            VERBOSE=true
            shift
            ;;
        --skip-build)
            SKIP_BUILD=true
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# =============================================================================
# Prerequisites Check
# =============================================================================

check_prerequisites() {
    log_section "Checking Prerequisites"

    local missing=0

    # Check cmake
    if command -v cmake &>/dev/null; then
        log_success "cmake found: $(cmake --version | head -1)"
    else
        log_error "cmake not found"
        missing=1
    fi

    # Check zenohd
    if "$ZENOHD" --version &>/dev/null; then
        log_success "zenohd found: $ZENOHD"
    else
        log_error "zenohd not found"
        log_info "Build with: just build-zenohd"
        missing=1
    fi

    # Check cargo
    if command -v cargo &>/dev/null; then
        log_success "cargo found: $(cargo --version)"
    else
        log_error "cargo not found"
        missing=1
    fi

    return $missing
}

# =============================================================================
# Build nano-ros-c Library
# =============================================================================

build_nano_ros_c() {
    log_section "Building nano-ros-c Library"

    cd "$PROJECT_ROOT"

    if cargo build -p nano-ros-c --release 2>&1 | tee "$(tmpfile cargo_build.txt)" | tail -5; then
        log_success "nano-ros-c library built"
    else
        log_error "nano-ros-c build failed"
        [ "$VERBOSE" = true ] && cat "$(tmpfile cargo_build.txt)"
        return 1
    fi

    # Verify library exists
    if [ -f "$PROJECT_ROOT/target/release/libnano_ros_c.a" ]; then
        log_success "Library found: target/release/libnano_ros_c.a"
    else
        log_error "Library not found after build"
        return 1
    fi
}

# =============================================================================
# Build C Examples
# =============================================================================

build_c_examples() {
    log_section "Building C Examples"

    local failed=0

    # Build native/c-talker
    log_info "Building native/c-talker..."
    local talker_dir="$PROJECT_ROOT/examples/native/c-talker"
    local talker_build="$talker_dir/build"

    rm -rf "$talker_build"
    mkdir -p "$talker_build"
    cd "$talker_build"

    if cmake -DNANO_ROS_ROOT="$PROJECT_ROOT" .. > "$(tmpfile talker_cmake.txt)" 2>&1 && \
       make > "$(tmpfile talker_make.txt)" 2>&1; then
        log_success "native/c-talker built"
    else
        log_error "native/c-talker build failed"
        [ "$VERBOSE" = true ] && cat "$(tmpfile talker_cmake.txt)" "$(tmpfile talker_make.txt)"
        failed=1
    fi

    # Build native/c-listener
    log_info "Building native/c-listener..."
    local listener_dir="$PROJECT_ROOT/examples/native/c-listener"
    local listener_build="$listener_dir/build"

    rm -rf "$listener_build"
    mkdir -p "$listener_build"
    cd "$listener_build"

    if cmake -DNANO_ROS_ROOT="$PROJECT_ROOT" .. > "$(tmpfile listener_cmake.txt)" 2>&1 && \
       make > "$(tmpfile listener_make.txt)" 2>&1; then
        log_success "native/c-listener built"
    else
        log_error "native/c-listener build failed"
        [ "$VERBOSE" = true ] && cat "$(tmpfile listener_cmake.txt)" "$(tmpfile listener_make.txt)"
        failed=1
    fi

    return $failed
}

# =============================================================================
# Test: C Talker -> C Listener
# =============================================================================

test_c_pubsub() {
    log_section "Test: C Talker -> C Listener"

    local talker_bin="$PROJECT_ROOT/examples/native/c-talker/build/c_talker"
    local listener_bin="$PROJECT_ROOT/examples/native/c-listener/build/c_listener"

    # Verify binaries exist
    if [ ! -x "$talker_bin" ]; then
        log_error "Talker binary not found: $talker_bin"
        return 1
    fi
    if [ ! -x "$listener_bin" ]; then
        log_error "Listener binary not found: $listener_bin"
        return 1
    fi

    # Start zenoh router
    log_info "Starting zenoh router..."
    pkill -x zenohd 2>/dev/null || true
    sleep 1
    "$ZENOHD" --listen tcp/127.0.0.1:7447 > "$(tmpfile zenohd.txt)" 2>&1 &
    local zenohd_pid=$!
    register_pid $zenohd_pid
    sleep 2

    if ! kill -0 $zenohd_pid 2>/dev/null; then
        log_error "Failed to start zenohd"
        cat "$(tmpfile zenohd.txt)"
        return 1
    fi
    log_success "zenohd started (PID: $zenohd_pid)"

    # Start C listener
    # Use stdbuf to disable stdout buffering for proper output capture
    log_info "Starting C listener..."
    stdbuf -oL -eL timeout 15 "$listener_bin" > "$(tmpfile listener.txt)" 2>&1 &
    local listener_pid=$!
    register_pid $listener_pid
    sleep 2

    # Start C talker
    log_info "Starting C talker..."
    stdbuf -oL -eL timeout 10 "$talker_bin" > "$(tmpfile talker.txt)" 2>&1 &
    local talker_pid=$!
    register_pid $talker_pid

    # Wait for communication
    log_info "Waiting for messages (timeout: 10s)..."

    local elapsed=0
    local max_wait=10
    while [ $elapsed -lt $max_wait ]; do
        # Check if listener received messages
        if grep -q "Received" "$(tmpfile listener.txt)" 2>/dev/null; then
            local count
            count=$(grep -c "Received" "$(tmpfile listener.txt)" 2>/dev/null || echo 0)
            if [ "$count" -ge 3 ]; then
                break
            fi
        fi
        sleep 1
        elapsed=$((elapsed + 1))
    done

    # Check that both processes initialized successfully
    local talker_init=false
    local listener_init=false

    if grep -q "Support initialized" "$(tmpfile talker.txt)" 2>/dev/null; then
        talker_init=true
    fi
    if grep -q "Support initialized" "$(tmpfile listener.txt)" 2>/dev/null; then
        listener_init=true
    fi

    # Check for received messages (may not work until timer bug is fixed)
    local received_count
    received_count=$(grep -c "Received" "$(tmpfile listener.txt)" 2>/dev/null || echo "0")
    # Ensure it's a clean integer
    received_count="${received_count%%[^0-9]*}"
    received_count="${received_count:-0}"

    if [ "$VERBOSE" = true ]; then
        echo ""
        echo "=== Talker Output ==="
        cat "$(tmpfile talker.txt)" 2>/dev/null || echo "(empty)"
        echo ""
        echo "=== Listener Output ==="
        cat "$(tmpfile listener.txt)" 2>/dev/null || echo "(empty)"
    fi

    # Require successful initialization and message exchange
    if [ "$talker_init" = true ] && [ "$listener_init" = true ] && [ "$received_count" -ge 3 ]; then
        log_success "C listener received $received_count messages from C talker!"
        return 0
    elif [ "$talker_init" = true ] && [ "$listener_init" = true ]; then
        log_error "C examples initialized but insufficient messages received (got: $received_count, expected: >= 3)"
        return 1
    else
        log_error "C examples failed to initialize"
        echo ""
        echo "=== Talker Output ==="
        cat "$(tmpfile talker.txt)" 2>/dev/null || echo "(empty)"
        echo ""
        echo "=== Listener Output ==="
        cat "$(tmpfile listener.txt)" 2>/dev/null || echo "(empty)"
        echo ""
        echo "=== zenohd Output ==="
        tail -20 "$(tmpfile zenohd.txt)" 2>/dev/null || echo "(empty)"
        return 1
    fi
}

# =============================================================================
# Main
# =============================================================================

log_section "C Integration Tests"

RESULT=0

# Check prerequisites
if ! check_prerequisites; then
    log_error "Prerequisites not met"
    exit 1
fi

# Build phase
if [ "$SKIP_BUILD" = false ]; then
    if ! build_nano_ros_c; then
        log_error "nano-ros-c build failed"
        exit 1
    fi

    if ! build_c_examples; then
        log_error "C examples build failed"
        exit 1
    fi
fi

# Run tests
test_c_pubsub || RESULT=1

# Summary
log_section "Test Summary"
if [ $RESULT -eq 0 ]; then
    log_success "All C tests passed!"
else
    log_error "Some C tests failed"
    log_info ""
    log_info "Troubleshooting:"
    log_info "  1. Check zenohd is running: pgrep -x zenohd"
    log_info "  2. Rebuild: ./tests/c-tests.sh"
    log_info "  3. Run with --verbose for detailed output"
fi

exit $RESULT
