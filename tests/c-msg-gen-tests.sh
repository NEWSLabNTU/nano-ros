#!/bin/bash
# Integration test for C message generation
#
# This script tests the full pipeline:
# 1. Build cargo-nano-ros
# 2. Build nros-c library
# 3. Run CMake on native-c-custom-msg example
# 4. Build and run the test executable
#
# Usage: ./tests/c-msg-gen-tests.sh
#
# Exit codes:
#   0 - All tests passed
#   1 - Test failure or error

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Clean up on exit
cleanup() {
    if [ -n "$BUILD_DIR" ] && [ -d "$BUILD_DIR" ]; then
        rm -rf "$BUILD_DIR"
    fi
}

trap cleanup EXIT

# ============================================================================
# Step 1: Build cargo-nano-ros
# ============================================================================

info "Building cargo-nano-ros..."
cd "$PROJECT_ROOT/packages/codegen/packages"

cargo build --release --package cargo-nano-ros

GENERATOR="$PROJECT_ROOT/packages/codegen/packages/target/release/cargo-nano-ros"
if [ ! -f "$GENERATOR" ]; then
    error "cargo-nano-ros not found at: $GENERATOR"
    exit 1
fi

info "cargo-nano-ros built successfully: $GENERATOR"

# ============================================================================
# Step 2: Build install-local (nros-c + nros-codegen-c + CMake package)
# ============================================================================

cd "$PROJECT_ROOT"
info "Running install-local..."
just install-local

INSTALL_DIR="$PROJECT_ROOT/build/install"
if [ ! -f "$INSTALL_DIR/lib/libnros_c_zenoh.a" ]; then
    error "install-local failed: libnros_c_zenoh.a not found at $INSTALL_DIR/lib/"
    exit 1
fi

info "install-local complete: $INSTALL_DIR/"

# ============================================================================
# Step 3: Configure native-c-custom-msg example
# ============================================================================

info "Configuring native-c-custom-msg example..."

EXAMPLE_DIR="$PROJECT_ROOT/examples/native/c/zenoh/custom-msg"
BUILD_DIR="$EXAMPLE_DIR/build"

# Clean any existing build
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

# Configure with CMake
cmake -DNanoRos_DIR="$PROJECT_ROOT/build/install/lib/cmake/NanoRos" -DCMAKE_BUILD_TYPE=Release ..

info "CMake configuration successful"

# ============================================================================
# Step 4: Build the example
# ============================================================================

info "Building native-c-custom-msg example..."

cmake --build . --parallel

info "Build successful"

# ============================================================================
# Step 5: Run the test executable
# ============================================================================

info "Running test executable..."

TEST_EXEC="$BUILD_DIR/test_messages"
if [ ! -f "$TEST_EXEC" ]; then
    error "Test executable not found at: $TEST_EXEC"
    exit 1
fi

# Run the test
OUTPUT=$("$TEST_EXEC" 2>&1)
RESULT=$?

echo "$OUTPUT"

if [ $RESULT -ne 0 ]; then
    error "Test executable failed with exit code: $RESULT"
    exit 1
fi

# Check for expected output
if echo "$OUTPUT" | grep -q "All tests passed"; then
    info "Test executable reported success"
else
    # Check for individual test results
    if echo "$OUTPUT" | grep -q "Temperature" && echo "$OUTPUT" | grep -q "SensorData"; then
        info "Found expected message types in output"
    else
        warn "Test output may not contain expected content"
    fi
fi

# ============================================================================
# Step 6: Verify generated files
# ============================================================================

info "Verifying generated files..."

GEN_DIR="$BUILD_DIR/nano_ros_c/native_c_custom_msg"

# Check for expected generated files
EXPECTED_FILES=(
    "msg/native_c_custom_msg_msg_temperature.h"
    "msg/native_c_custom_msg_msg_temperature.c"
    "msg/native_c_custom_msg_msg_sensor_data.h"
    "msg/native_c_custom_msg_msg_sensor_data.c"
    "native_c_custom_msg.h"
)

for FILE in "${EXPECTED_FILES[@]}"; do
    if [ ! -f "$GEN_DIR/$FILE" ]; then
        error "Expected file not found: $GEN_DIR/$FILE"
        exit 1
    fi
done

info "All expected files generated"

# ============================================================================
# Summary
# ============================================================================

echo ""
echo "=============================================="
echo -e "${GREEN}All C message generation tests passed!${NC}"
echo "=============================================="
echo ""
echo "Generated files:"
find "$GEN_DIR" -type f -name "*.h" -o -name "*.c" | sort | while read f; do
    echo "  - ${f#$GEN_DIR/}"
done
echo ""

exit 0
