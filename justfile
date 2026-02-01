# Common clippy lints for real-time safety
CLIPPY_LINTS := "-D warnings -D clippy::infinite_iter -D clippy::while_immutable_condition -D clippy::never_loop -D clippy::empty_loop -D clippy::unconditional_recursion -W clippy::large_stack_arrays -W clippy::large_types_passed_by_value"

default:
    @just --list

# =============================================================================
# Entry Points
# =============================================================================

# Build everything: workspace (native + embedded), C++ bindings, and all examples
build: build-workspace build-workspace-embedded build-cpp build-examples build-examples-cpp
    @echo "All builds completed!"

# Format everything: workspace, C++, and all examples
format: format-workspace format-cpp format-examples format-examples-cpp
    @echo "All formatting completed!"

# Check everything: formatting, clippy (native + embedded + features), C++, and all examples
check: check-workspace check-workspace-embedded check-workspace-features check-cpp check-examples check-examples-cpp
    @echo "All checks passed!"

# Run quick tests: workspace unit tests only (no integration tests)
test-quick: test-workspace
    @echo "Quick tests passed!"

# Test everything: workspace tests, Miri, QEMU, Rust integration, and shell integration tests
test: test-workspace test-miri test-qemu test-rust test-integration
    @echo "All tests passed!"

# Run code quality checks (formatting + clippy + unit tests) - no integration tests
# Runs all checks even if some fail, then reports all failures at the end
quality:
    #!/usr/bin/env bash
    set +e  # Don't exit on first error
    failed=0

    echo "=== Format Check ==="
    cargo +nightly fmt --check
    if [ $? -ne 0 ]; then
        echo "[FAIL] Format check FAILED"
        failed=1
    else
        echo "[OK] Format check passed"
    fi

    echo ""
    echo "=== Clippy (workspace, no_std) ==="
    cargo clippy --workspace --no-default-features \
        --exclude nano-ros-c -- {{CLIPPY_LINTS}}
    if [ $? -ne 0 ]; then
        echo "[FAIL] Clippy (workspace) FAILED"
        failed=1
    else
        echo "[OK] Clippy (workspace) passed"
    fi

    echo ""
    echo "=== Clippy (embedded target) ==="
    cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zenoh-pico-shim-sys \
        --exclude nano-ros-tests \
        --exclude nano-ros-c -- {{CLIPPY_LINTS}}
    if [ $? -ne 0 ]; then
        echo "[FAIL] Clippy (embedded) FAILED"
        failed=1
    else
        echo "[OK] Clippy (embedded) passed"
    fi

    echo ""
    echo "=== Unit Tests ==="
    # Exclude nano-ros-tests crate which contains integration tests requiring external setup
    cargo nextest run --workspace --exclude nano-ros-tests --no-fail-fast
    if [ $? -ne 0 ]; then
        echo "[FAIL] Unit tests FAILED"
        failed=1
    else
        echo "[OK] Unit tests passed"
    fi

    echo ""
    echo "=== Miri (UB detection) ==="
    miri_failed=0
    cargo +nightly miri test -p nano-ros-serdes || miri_failed=1
    cargo +nightly miri test -p nano-ros-core || miri_failed=1
    if [ $miri_failed -ne 0 ]; then
        echo "[FAIL] Miri FAILED"
        failed=1
    else
        echo "[OK] Miri passed"
    fi

    echo ""
    echo "=== QEMU Examples ==="
    qemu_failed=0
    (cd examples/qemu-rs-test && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}) || qemu_failed=1
    (cd examples/qemu-rs-lan9118 && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}) || qemu_failed=1
    (cd examples/qemu-rs-talker && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}) || qemu_failed=1
    (cd examples/qemu-rs-listener && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}) || qemu_failed=1
    if [ $qemu_failed -ne 0 ]; then
        echo "[FAIL] QEMU examples FAILED"
        failed=1
    else
        echo "[OK] QEMU examples passed"
    fi

    echo ""
    if [ $failed -ne 0 ]; then
        echo "[FAIL] Quality checks FAILED - see errors above"
        exit 1
    else
        echo "[OK] All quality checks passed!"
    fi

# Run full CI suite (quality + all integration tests)
ci: check test
    @echo "Full CI suite passed!"

# =============================================================================
# Workspace
# =============================================================================

# Build workspace (no_std, native)
# Excludes nano-ros-c which currently requires std
build-workspace:
    cargo build --workspace --no-default-features --exclude nano-ros-c

# Build workspace for embedded target (Cortex-M4F)
# Excludes zenoh-pico-shim-sys which requires native system headers for CMake build
# Excludes nano-ros-tests which requires std (test framework dependencies)
# Excludes nano-ros-c which currently requires std
build-workspace-embedded:
    cargo build --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zenoh-pico-shim-sys \
        --exclude nano-ros-tests \
        --exclude nano-ros-c

# Format workspace code
format-workspace:
    cargo +nightly fmt

# Check workspace: formatting and clippy (no_std, native)
# Excludes nano-ros-c which currently requires std
check-workspace:
    cargo +nightly fmt --check
    cargo clippy --workspace --no-default-features --exclude nano-ros-c -- {{CLIPPY_LINTS}}

# Check workspace for embedded target (Cortex-M4F)
# Excludes zenoh-pico-shim-sys which requires native system headers for CMake build
# Excludes nano-ros-tests which requires std (test framework dependencies)
# Excludes nano-ros-c which currently requires std
check-workspace-embedded:
    @echo "Checking workspace for embedded target..."
    cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zenoh-pico-shim-sys \
        --exclude nano-ros-tests \
        --exclude nano-ros-c -- {{CLIPPY_LINTS}}

# Check workspace with various feature combinations
check-workspace-features:
    @echo "Checking feature combinations..."
    @echo "  - transport: rtic + sync-critical-section"
    cargo clippy -p nano-ros-transport --no-default-features --features "rtic,sync-critical-section" --target thumbv7em-none-eabihf -- {{CLIPPY_LINTS}}
    @echo "  - node: rtic"
    cargo clippy -p nano-ros-node --no-default-features --features "rtic" --target thumbv7em-none-eabihf -- {{CLIPPY_LINTS}}
    @echo "  - zenoh transport (std)"
    cargo clippy -p nano-ros-transport --features "zenoh,std" -- {{CLIPPY_LINTS}}
    @echo "All feature checks passed!"

# Run workspace tests (requires std)
test-workspace:
    cargo nextest run --workspace --no-fail-fast

# Run Miri to detect undefined behavior
test-miri:
    @echo "Running Miri on safe crates..."
    cargo +nightly miri test -p nano-ros-serdes
    cargo +nightly miri test -p nano-ros-core
    @echo "Miri checks passed!"

# =============================================================================
# Examples
# =============================================================================

# Build all examples
build-examples: build-examples-native build-examples-embedded build-examples-qemu
    @echo "All examples built!"

# Format all examples
format-examples: format-examples-native format-examples-embedded format-examples-qemu
    @echo "All examples formatted!"

# Check all examples
check-examples: check-examples-native check-examples-embedded check-examples-qemu
    @echo "All examples check passed!"

# =============================================================================
# Examples - Native
# =============================================================================

# Build native examples
build-examples-native:
    @echo "Building native examples..."
    cd examples/native-rs-talker && cargo build
    cd examples/native-rs-listener && cargo build
    cd examples/native-rs-service-server && cargo build
    cd examples/native-rs-service-client && cargo build
    cd examples/native-rs-action-server && cargo build
    cd examples/native-rs-action-client && cargo build

# Format native examples
format-examples-native:
    @echo "Formatting native examples..."
    cd examples/native-rs-talker && cargo +nightly fmt
    cd examples/native-rs-listener && cargo +nightly fmt
    cd examples/native-rs-service-server && cargo +nightly fmt
    cd examples/native-rs-service-client && cargo +nightly fmt
    cd examples/native-rs-action-server && cargo +nightly fmt
    cd examples/native-rs-action-client && cargo +nightly fmt

# Check native examples
check-examples-native:
    @echo "Checking native examples..."
    cd examples/native-rs-talker && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}}
    cd examples/native-rs-listener && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}}
    cd examples/native-rs-service-server && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}}
    cd examples/native-rs-service-client && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}}
    cd examples/native-rs-action-server && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}}
    cd examples/native-rs-action-client && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}}

# =============================================================================
# Examples - Embedded (STM32F4)
# =============================================================================

# Build embedded examples
build-examples-embedded:
    @echo "Building embedded examples..."
    cd examples/stm32f4-rs-rtic && cargo build --release
    cd examples/stm32f4-rs-embassy && cargo build --release
    cd examples/stm32f4-rs-polling && cargo build --release
    cd examples/stm32f4-rs-smoltcp && cargo build --release

# Format embedded examples
format-examples-embedded:
    @echo "Formatting embedded examples..."
    cd examples/stm32f4-rs-rtic && cargo +nightly fmt
    cd examples/stm32f4-rs-embassy && cargo +nightly fmt
    cd examples/stm32f4-rs-polling && cargo +nightly fmt
    cd examples/stm32f4-rs-smoltcp && cargo +nightly fmt

# Check embedded examples
check-examples-embedded:
    @echo "Checking embedded examples..."
    cd examples/stm32f4-rs-rtic && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}
    cd examples/stm32f4-rs-embassy && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}
    cd examples/stm32f4-rs-polling && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}
    cd examples/stm32f4-rs-smoltcp && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}

# Show embedded example binary sizes
size-examples-embedded: build-examples-embedded
    @echo ""
    @echo "Binary sizes (release):"
    @echo "======================="
    @size examples/stm32f4-rs-rtic/target/thumbv7em-none-eabihf/release/stm32f4-rs-rtic-example 2>/dev/null || echo "RTIC: build failed"
    @size examples/stm32f4-rs-embassy/target/thumbv7em-none-eabihf/release/stm32f4-rs-embassy-example 2>/dev/null || echo "Embassy: build failed"
    @size examples/stm32f4-rs-polling/target/thumbv7em-none-eabihf/release/stm32f4-rs-polling-example 2>/dev/null || echo "Polling: build failed"
    @size examples/stm32f4-rs-smoltcp/target/thumbv7em-none-eabihf/release/stm32f4-rs-smoltcp 2>/dev/null || echo "stm32f4-rs-smoltcp: build failed"

# Clean embedded example build artifacts
clean-examples-embedded:
    rm -rf examples/stm32f4-rs-rtic/target
    rm -rf examples/stm32f4-rs-embassy/target
    rm -rf examples/stm32f4-rs-polling/target
    rm -rf examples/stm32f4-rs-smoltcp/target
    @echo "Embedded example build artifacts cleaned"

# Clean native example build artifacts
clean-examples-native:
    rm -rf examples/native-rs-talker/target
    rm -rf examples/native-rs-listener/target
    rm -rf examples/native-rs-service-server/target
    rm -rf examples/native-rs-service-client/target
    rm -rf examples/native-rs-action-server/target
    rm -rf examples/native-rs-action-client/target
    @echo "Native example build artifacts cleaned"

# Clean QEMU example build artifacts
clean-examples-qemu:
    rm -rf examples/qemu-rs-test/target
    rm -rf examples/qemu-rs-lan9118/target
    rm -rf examples/qemu-rs-talker/target
    rm -rf examples/qemu-rs-listener/target
    @echo "QEMU example build artifacts cleaned"

# Clean all example build artifacts
clean-examples: clean-examples-native clean-examples-embedded clean-examples-qemu clean-examples-cpp clean-examples-c
    @echo "All example build artifacts cleaned"

# =============================================================================
# Examples - Zephyr (native_sim)
# =============================================================================

# Zephyr workspace path (symlink or sibling directory)
ZEPHYR_WORKSPACE := if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else { "../nano-ros-workspace" }

# Build Zephyr examples (talker and listener to separate directories)
build-zephyr:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr examples in $WORKSPACE..."
    cd "$WORKSPACE"
    echo "  Building zephyr-rs-talker -> build-talker/"
    west build -b native_sim/native/64 -d build-talker -p auto nano-ros/examples/zephyr-rs-talker
    echo "  Building zephyr-rs-listener -> build-listener/"
    west build -b native_sim/native/64 -d build-listener -p auto nano-ros/examples/zephyr-rs-listener
    echo "Zephyr examples built successfully!"

# Clean Zephyr build directories
clean-zephyr:
    #!/usr/bin/env bash
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    rm -rf "$WORKSPACE/build-talker" "$WORKSPACE/build-listener"
    echo "Zephyr build directories cleaned"

# Force rebuild Zephyr examples
rebuild-zephyr: clean-zephyr build-zephyr

# =============================================================================
# Examples - QEMU (Cortex-M3)
# =============================================================================

# Build QEMU examples
build-examples-qemu:
    @echo "Building QEMU examples..."
    cd examples/qemu-rs-test && cargo build --release
    cd examples/qemu-rs-lan9118 && cargo build --release
    cd examples/qemu-rs-talker && cargo build --release
    cd examples/qemu-rs-listener && cargo build --release

# Format QEMU examples
format-examples-qemu:
    @echo "Formatting QEMU examples..."
    cd examples/qemu-rs-test && cargo +nightly fmt
    cd examples/qemu-rs-lan9118 && cargo +nightly fmt
    cd examples/qemu-rs-talker && cargo +nightly fmt
    cd examples/qemu-rs-listener && cargo +nightly fmt

# Check QEMU examples
check-examples-qemu:
    @echo "Checking QEMU examples..."
    cd examples/qemu-rs-test && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}
    cd examples/qemu-rs-lan9118 && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}
    cd examples/qemu-rs-talker && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}
    cd examples/qemu-rs-listener && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}

# Run all QEMU tests (non-networked)
test-qemu: test-qemu-basic test-qemu-lan9118
    @echo "All QEMU tests passed!"

# Build zenoh-pico for ARM Cortex-M3 (required for qemu-rs-talker/listener)
build-zenoh-pico-arm:
    @echo "Building zenoh-pico for ARM Cortex-M3..."
    ./scripts/qemu/build-zenoh-pico.sh
    @echo "zenoh-pico built at: build/qemu-zenoh-pico/libzenohpico.a"

# Clean zenoh-pico ARM build
clean-zenoh-pico-arm:
    ./scripts/qemu/build-zenoh-pico.sh --clean

# Run basic QEMU test (nano-ros serialization on Cortex-M3)
test-qemu-basic: build-examples-qemu
    @echo "Running QEMU basic test (lm3s6965evb)..."
    qemu-system-arm \
        -cpu cortex-m3 \
        -machine lm3s6965evb \
        -nographic \
        -semihosting-config enable=on,target=native \
        -kernel examples/qemu-rs-test/target/thumbv7m-none-eabi/release/qemu-rs-test

# Run LAN9118 Ethernet driver test (mps2-an385)
test-qemu-lan9118: build-examples-qemu
    @echo "Running QEMU LAN9118 test (mps2-an385)..."
    qemu-system-arm \
        -cpu cortex-m3 \
        -machine mps2-an385 \
        -nographic \
        -semihosting-config enable=on,target=native \
        -kernel examples/qemu-rs-lan9118/target/thumbv7m-none-eabi/release/qemu-rs-lan9118

# Check if QEMU is installed
check-qemu:
    @which qemu-system-arm > /dev/null || (echo "Error: qemu-system-arm not found. Install with: sudo apt install qemu-system-arm" && exit 1)
    @echo "QEMU ARM is installed"

# Setup QEMU TAP network bridge (requires sudo)
setup-qemu-network:
    sudo ./scripts/qemu/setup-network.sh

# Teardown QEMU TAP network bridge (requires sudo)
teardown-qemu-network:
    sudo ./scripts/qemu/setup-network.sh --down

# Show QEMU network status
status-qemu-network:
    ./scripts/qemu/setup-network.sh --status

# Test QEMU zenoh-pico communication (requires zenohd + TAP network)
# This runs qemu-rs-talker and qemu-rs-listener via zenohd
test-qemu-zenoh: build-zenoh-pico-arm build-examples-qemu
    #!/usr/bin/env bash
    set -e
    echo "============================================"
    echo "  QEMU zenoh-pico Communication Test"
    echo "============================================"
    echo ""
    echo "Prerequisites:"
    echo "  1. TAP network: sudo ./scripts/qemu/setup-network.sh"
    echo "  2. zenohd running: zenohd --listen tcp/0.0.0.0:7447"
    echo ""
    echo "This test requires QEMU 7.0+ for reliable TAP networking."
    echo "Ubuntu 22.04 ships QEMU 6.2.0 which has TAP issues."
    echo ""
    echo "Current QEMU version:"
    qemu-system-arm --version | head -1
    echo ""
    echo "To run manually:"
    echo "  Terminal 1: zenohd --listen tcp/0.0.0.0:7447"
    echo "  Terminal 2: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary examples/qemu-rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker"
    echo "  Terminal 3: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 --binary examples/qemu-rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener"
    echo ""
    echo "Binaries built at:"
    echo "  examples/qemu-rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker"
    echo "  examples/qemu-rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener"
    echo ""
    echo "Note: Automated test not yet implemented (requires QEMU 7.0+)"
    echo "============================================"

# Launch QEMU mps2-an385 with networking (example)
run-qemu-networked TAP="tap-qemu0" IP="192.0.2.10" BINARY="":
    ./scripts/qemu/launch-mps2-an385.sh --tap {{TAP}} --ip {{IP}} --binary {{BINARY}}

# Show QEMU help
qemu-help:
    @echo "QEMU Bare-Metal Testing"
    @echo "======================="
    @echo ""
    @echo "Prerequisites:"
    @echo "  1. Install QEMU: sudo apt install qemu-system-arm"
    @echo "  2. Set up network: sudo ./scripts/qemu/setup-network.sh"
    @echo ""
    @echo "Build & Test (non-networked):"
    @echo "  just build-examples-qemu     # Build all QEMU examples"
    @echo "  just test-qemu               # Run all QEMU tests (no network)"
    @echo "  just test-qemu-basic         # Run basic serialization test"
    @echo "  just test-qemu-lan9118       # Run LAN9118 driver test"
    @echo ""
    @echo "Zenoh-pico (networked):"
    @echo "  just build-zenoh-pico-arm    # Build zenoh-pico for ARM Cortex-M3"
    @echo "  just test-qemu-zenoh         # Show zenoh test instructions"
    @echo ""
    @echo "Network Setup:"
    @echo "  just setup-qemu-network      # Create TAP bridge (requires sudo)"
    @echo "  just teardown-qemu-network   # Remove TAP bridge (requires sudo)"
    @echo "  just status-qemu-network     # Show network status"
    @echo ""
    @echo "Manual QEMU Launch:"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --binary app.elf"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary app.elf"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --gdb --binary app.elf"
    @echo ""
    @echo "For more details: ./scripts/qemu/launch-mps2-an385.sh --help"

# Run multi-node test (QEMU + native)
test-multi-node:
    ./scripts/run-multi-node-test.sh

# =============================================================================
# Static Analysis
# =============================================================================

# Analyze stack usage (requires nightly)
analyze-stack:
    @echo "Analyzing stack usage for RTIC example..."
    cd examples/stm32f4-rs-rtic && \
        RUSTFLAGS="-Z emit-stack-sizes" cargo +nightly build --release 2>&1 | head -20
    @echo ""
    @echo "Note: For full call graph analysis, install cargo-call-stack:"
    @echo "  cargo +nightly install cargo-call-stack"
    @echo "  cd examples/stm32f4-rs-rtic && cargo +nightly call-stack --release"

# Run all static analysis checks (Miri UB detection)
static-analysis: test-miri
    @echo ""
    @echo "All static analysis checks passed!"

# =============================================================================
# C++ Bindings
# =============================================================================

# Build C++ bindings (nano-ros-cpp)
build-cpp:
    @echo "Building C++ bindings..."
    cd crates/nano-ros-cpp && cmake -B build && cmake --build build

# Build C++ bindings (release)
build-cpp-release:
    @echo "Building C++ bindings (release)..."
    cd crates/nano-ros-cpp && cmake -B build -DCMAKE_BUILD_TYPE=Release && cmake --build build

# Format C++ code
format-cpp:
    @echo "Formatting C++ code..."
    @which clang-format > /dev/null || (echo "Error: clang-format not found. Install with: sudo apt install clang-format" && exit 1)
    find crates/nano-ros-cpp/cpp crates/nano-ros-cpp/include/nano_ros \
        -name '*.cpp' -o -name '*.hpp' -o -name '*.h' | \
        xargs clang-format -i --style=file:crates/nano-ros-cpp/.clang-format
    @echo "C++ code formatted"

# Check C++ formatting and run clang-tidy lints
check-cpp: _check-cpp-format _check-cpp-tidy
    @echo "C++ checks passed"

# Check C++ formatting only (does not modify files)
# Note: Excludes generated message headers (std_msgs, builtin_interfaces) which are auto-generated
_check-cpp-format:
    @echo "Checking C++ formatting..."
    @which clang-format > /dev/null || (echo "Error: clang-format not found. Install with: sudo apt install clang-format" && exit 1)
    find crates/nano-ros-cpp/cpp crates/nano-ros-cpp/include/nano_ros \
        -name '*.cpp' -o -name '*.hpp' -o -name '*.h' | \
        xargs clang-format --dry-run --Werror --style=file:crates/nano-ros-cpp/.clang-format
    @echo "C++ formatting check passed"

# Run clang-tidy on C++ code (requires build for compile_commands.json)
_check-cpp-tidy:
    @echo "Running clang-tidy..."
    @which clang-tidy > /dev/null || (echo "Error: clang-tidy not found. Install with: sudo apt install clang-tidy" && exit 1)
    @test -f crates/nano-ros-cpp/build/compile_commands.json || (echo "Error: compile_commands.json not found. Run 'just build-cpp' first." && exit 1)
    cd crates/nano-ros-cpp && clang-tidy -p build cpp/*.cpp
    @echo "clang-tidy check passed"

# Clean C++ bindings build
clean-cpp:
    rm -rf crates/nano-ros-cpp/build
    @echo "C++ bindings build cleaned"

# =============================================================================
# Examples - C++
# =============================================================================

# Build C++ examples
build-examples-cpp: build-cpp
    @echo "Building C++ examples..."
    cd examples/native-cpp-talker && cmake -B build && cmake --build build
    cd examples/native-cpp-listener && cmake -B build && cmake --build build
    cd examples/native-cpp-custom-msg && cmake -B build && cmake --build build
    cd examples/native-cpp-service-server && cmake -B build && cmake --build build
    cd examples/native-cpp-service-client && cmake -B build && cmake --build build

# Format C++ examples
format-examples-cpp:
    @echo "Formatting C++ examples..."
    @which clang-format > /dev/null || (echo "Error: clang-format not found." && exit 1)
    find examples/native-cpp-talker/src examples/native-cpp-listener/src examples/native-cpp-custom-msg/src \
        examples/native-cpp-service-server/src examples/native-cpp-service-client/src -name '*.cpp' | \
        xargs clang-format -i --style=file:crates/nano-ros-cpp/.clang-format

# Check C++ examples
check-examples-cpp:
    @echo "Checking C++ examples..."
    @which clang-format > /dev/null || (echo "Error: clang-format not found." && exit 1)
    find examples/native-cpp-talker/src examples/native-cpp-listener/src examples/native-cpp-custom-msg/src \
        examples/native-cpp-service-server/src examples/native-cpp-service-client/src -name '*.cpp' | \
        xargs clang-format --dry-run --Werror --style=file:crates/nano-ros-cpp/.clang-format

# Clean C++ examples build
clean-examples-cpp:
    rm -rf examples/native-cpp-talker/build examples/native-cpp-listener/build examples/native-cpp-custom-msg/build \
        examples/native-cpp-service-server/build examples/native-cpp-service-client/build
    @echo "C++ examples build cleaned"

# Run C++ talker (requires zenohd)
run-native-cpp-talker:
    @echo "Running C++ talker (requires zenohd)..."
    examples/native-cpp-talker/build/cpp_talker

# Run C++ listener (requires zenohd)
run-native-cpp-listener:
    @echo "Running C++ listener (requires zenohd)..."
    examples/native-cpp-listener/build/cpp_listener

# Run C++ custom message example (requires zenohd)
run-native-cpp-custom-msg:
    @echo "Running C++ custom message example (requires zenohd)..."
    examples/native-cpp-custom-msg/build/cpp_custom_msg

# Run C++ service server (requires zenohd)
run-native-cpp-service-server:
    @echo "Running C++ service server (requires zenohd)..."
    examples/native-cpp-service-server/build/cpp_service_server

# Run C++ service client (requires zenohd + service server)
# NOTE: Service client is not yet supported in transport layer
run-native-cpp-service-client:
    @echo "Running C++ service client (requires zenohd + service server)..."
    @echo "NOTE: Service client creation will fail - not yet implemented in transport layer"
    examples/native-cpp-service-client/build/cpp_service_client

# =============================================================================
# Zenoh
# =============================================================================

# Build zenoh transport
build-zenoh:
    cargo build -p nano-ros-transport --features zenoh,std

# Check zenoh transport
check-zenoh:
    cargo clippy -p nano-ros-transport --features zenoh,std -- {{CLIPPY_LINTS}}

# Build zenoh-pico C library (standalone, for debugging)
build-zenoh-pico:
    @echo "Building zenoh-pico..."
    cd crates/zenoh-pico-shim-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: crates/zenoh-pico-shim-sys/zenoh-pico/build"

# Test zenoh-pico-shim (requires zenohd running)
test-zenoh-shim:
    @echo "Testing zenoh-pico-shim (requires: zenohd --listen tcp/127.0.0.1:7447)"
    cargo test -p zenoh-pico-shim --features "posix std" -- --test-threads=1

# =============================================================================
# Integration Tests
# =============================================================================

# Run all integration tests (Rust-based, requires zenohd)
test-integration:
    cargo test -p nano-ros-tests --tests -- --nocapture

# Run Zephyr C examples test (requires west workspace + TAP network)
test-zephyr-c:
    ./tests/zephyr/run-c.sh

# =============================================================================
# Rust Integration Tests (crates/nano-ros-tests)
# =============================================================================

# Run all Rust integration tests
test-rust:
    cargo test -p nano-ros-tests --tests -- --nocapture

# Run Rust emulator tests (QEMU Cortex-M3)
test-rust-emulator:
    cargo test -p nano-ros-tests --test emulator -- --nocapture

# Run Rust native pub/sub tests
test-rust-nano2nano:
    cargo test -p nano-ros-tests --test nano2nano -- --nocapture

# Run Rust platform detection tests
test-rust-platform:
    cargo test -p nano-ros-tests --test platform -- --nocapture

# Run Rust RMW interop tests (requires ROS 2 + rmw_zenoh_cpp)
test-rust-rmw-interop:
    cargo test -p nano-ros-tests --test rmw_interop -- --nocapture

# Run Rust Zephyr tests (requires west workspace + bridge network)
# Use test-rust-zephyr-full to force rebuild before testing
test-rust-zephyr:
    cargo test -p nano-ros-tests --test zephyr -- --nocapture

# Run Rust Zephyr tests with rebuild
test-rust-zephyr-full: build-zephyr
    cargo test -p nano-ros-tests --test zephyr -- --nocapture

# Run Rust tests via wrapper script (with nice output)
test-rust-full:
    ./tests/rust-tests.sh

# =============================================================================
# C Integration Tests
# =============================================================================

# Run C message generation tests (full integration: build + generate + compile + run)
test-c-msg-gen:
    ./tests/c-msg-gen-tests.sh

# Run C codegen Rust unit tests (in cargo-nano-ros)
test-rust-c-codegen:
    cd colcon-nano-ros/packages && cargo test -p cargo-nano-ros --test test_generate_c -- --nocapture

# Run all C codegen tests (unit + integration)
test-c-codegen: test-rust-c-codegen test-c-msg-gen
    @echo "All C codegen tests passed!"

# Run C integration tests (native-c-talker/listener)
test-c:
    ./tests/c-tests.sh

# Run C integration tests with verbose output
test-c-verbose:
    ./tests/c-tests.sh --verbose

# Build C examples only (no tests)
build-examples-c:
    @echo "Building nano-ros-c library..."
    cargo build -p nano-ros-c --release
    @echo "Building native-c-talker..."
    cd examples/native-c-talker && rm -rf build && mkdir -p build && cd build && cmake .. && make
    @echo "Building native-c-listener..."
    cd examples/native-c-listener && rm -rf build && mkdir -p build && cd build && cmake .. && make
    @echo "C examples built!"

# Clean C examples build
clean-examples-c:
    rm -rf examples/native-c-talker/build examples/native-c-listener/build
    @echo "C examples build cleaned"

# =============================================================================
# Message Bindings
# =============================================================================

# Install cargo-nano-ros (requires ROS 2 environment)
install-cargo-nano-ros:
    @echo "Installing cargo-nano-ros..."
    cargo install --path colcon-nano-ros/packages/cargo-nano-ros --locked

# Regenerate bindings in all examples (requires ROS 2 environment + cargo-nano-ros)
generate-bindings:
    @echo "Regenerating bindings in all examples..."
    @echo "Note: Requires ROS 2 environment sourced and cargo-nano-ros installed"
    cd examples/native-rs-talker && cargo nano-ros generate
    cd examples/native-rs-listener && cargo nano-ros generate
    cd examples/native-rs-service-server && cargo nano-ros generate
    cd examples/native-rs-service-client && cargo nano-ros generate
    cd examples/native-rs-action-server && cargo nano-ros generate
    cd examples/native-rs-action-client && cargo nano-ros generate
    cd examples/qemu-rs-test && cargo nano-ros generate
    cd examples/zephyr-rs-talker && cargo nano-ros generate
    cd examples/zephyr-rs-listener && cargo nano-ros generate
    @echo "All bindings regenerated!"

# =============================================================================
# Setup & Cleanup
# =============================================================================

# Install toolchains and tools
setup:
    @echo "=== Installing Rust toolchains ==="
    rustup toolchain install stable
    rustup toolchain install nightly
    rustup component add rustfmt clippy rust-src
    rustup component add --toolchain nightly rustfmt miri rust-src
    rustup target add thumbv7em-none-eabihf
    rustup target add thumbv7m-none-eabi
    @echo ""
    @echo "=== Installing cargo tools ==="
    cargo install cargo-nextest --locked
    cargo install --path colcon-nano-ros/packages/cargo-nano-ros --locked
    @echo ""
    @echo "=== Checking system dependencies ==="
    @which arm-none-eabi-gcc > /dev/null 2>&1 || (echo "WARNING: arm-none-eabi-gcc not found." && echo "For embedded development, install with: sudo apt install gcc-arm-none-eabi" && echo "")
    @which qemu-system-arm > /dev/null 2>&1 || (echo "WARNING: qemu-system-arm not found." && echo "For QEMU testing, install with: sudo apt install qemu-system-arm" && echo "")
    @which cmake > /dev/null 2>&1 || (echo "WARNING: cmake not found." && echo "For C++ bindings, install with: sudo apt install cmake" && echo "")
    @which clang-format > /dev/null 2>&1 || (echo "WARNING: clang-format not found." && echo "For C++ formatting, install with: sudo apt install clang-format" && echo "")
    @which clang-tidy > /dev/null 2>&1 || (echo "WARNING: clang-tidy not found." && echo "For C++ linting, install with: sudo apt install clang-tidy" && echo "")
    @echo "Setup complete!"

# Setup all network bridges (QEMU + Zephyr, requires sudo)
setup-network:
    @echo "Setting up network bridges..."
    sudo ./scripts/qemu/setup-network.sh
    sudo ./scripts/zephyr/setup-network.sh
    @echo "All network bridges configured!"

# Teardown all network bridges (requires sudo)
teardown-network:
    @echo "Tearing down network bridges..."
    sudo ./scripts/qemu/setup-network.sh --down
    sudo ./scripts/zephyr/setup-network.sh --down
    @echo "All network bridges removed!"

# Generate documentation
doc:
    cargo doc --no-deps --open

# Clean all build artifacts created by `just build`
clean: clean-cpp clean-examples
    cargo clean
    @echo "All build artifacts cleaned"

# Show Zephyr build instructions
zephyr-help:
    @echo "Zephyr Examples"
    @echo "==============="
    @echo ""
    @echo "Prerequisites:"
    @echo "  1. Set up Zephyr workspace: ./scripts/zephyr/setup.sh"
    @echo "  2. Set up bridge network:   sudo ./scripts/zephyr/setup-network.sh"
    @echo ""
    @echo "Build examples:"
    @echo "  just build-zephyr       # Build talker and listener"
    @echo "  just rebuild-zephyr     # Clean and rebuild"
    @echo "  just clean-zephyr       # Remove build directories"
    @echo ""
    @echo "Run tests:"
    @echo "  just test-rust-zephyr      # Run tests (uses existing binaries)"
    @echo "  just test-rust-zephyr-full # Rebuild and run tests"
    @echo ""
    @echo "Manual build (from Zephyr workspace):"
    @echo "  west build -b native_sim/native/64 -d build-talker nano-ros/examples/zephyr-rs-talker"
    @echo "  west build -b native_sim/native/64 -d build-listener nano-ros/examples/zephyr-rs-listener"

# =============================================================================
# Docker
# =============================================================================

# Build Docker image for QEMU ARM development
docker-build:
    @echo "Building Docker image..."
    docker build -t nano-ros-qemu -f docker/Dockerfile.qemu-arm .
    @echo "Docker image 'nano-ros-qemu' built successfully!"

# Start Docker container with interactive shell
docker-shell:
    @echo "Starting Docker container..."
    docker run -it --rm \
        -v $(pwd):/work \
        -v nano-ros-cargo-registry:/root/.cargo/registry \
        -v nano-ros-cargo-git:/root/.cargo/git \
        nano-ros-qemu

# Start Docker container with TAP networking support
docker-shell-network:
    @echo "Starting Docker container with networking support..."
    docker run -it --rm \
        --cap-add=NET_ADMIN \
        --device=/dev/net/tun \
        -v $(pwd):/work \
        -v nano-ros-cargo-registry:/root/.cargo/registry \
        -v nano-ros-cargo-git:/root/.cargo/git \
        nano-ros-qemu

# Run QEMU test inside Docker container
docker-test-qemu: docker-build
    @echo "Running QEMU test inside Docker..."
    docker run --rm \
        -v $(pwd):/work \
        nano-ros-qemu \
        bash -c "cd /work && just test-qemu-basic"

# Run QEMU build inside Docker container
docker-build-qemu: docker-build
    @echo "Building QEMU examples inside Docker..."
    docker run --rm \
        -v $(pwd):/work \
        -v nano-ros-cargo-registry:/root/.cargo/registry \
        -v nano-ros-cargo-git:/root/.cargo/git \
        nano-ros-qemu \
        bash -c "cd /work && just build-examples-qemu"

# Start Docker Compose services (QEMU + zenohd)
docker-up:
    docker compose -f docker/docker-compose.yml up -d

# Stop Docker Compose services
docker-down:
    docker compose -f docker/docker-compose.yml down

# Execute command in running Docker container
docker-exec CMD="bash":
    docker compose -f docker/docker-compose.yml exec qemu {{CMD}}

# Show Docker help
docker-help:
    @echo "Docker Development Environment"
    @echo "==============================="
    @echo ""
    @echo "Build Docker image:"
    @echo "  just docker-build         # Build nano-ros-qemu image"
    @echo ""
    @echo "Interactive shell:"
    @echo "  just docker-shell         # Start container with shell"
    @echo "  just docker-shell-network # Start with TAP networking support"
    @echo ""
    @echo "Run commands in Docker:"
    @echo "  just docker-test-qemu     # Run QEMU tests in Docker"
    @echo "  just docker-build-qemu    # Build QEMU examples in Docker"
    @echo ""
    @echo "Docker Compose (QEMU + zenohd):"
    @echo "  just docker-up            # Start services"
    @echo "  just docker-down          # Stop services"
    @echo "  just docker-exec          # Execute command in container"
    @echo ""
    @echo "Benefits:"
    @echo "  - QEMU 7.2 (Debian bookworm) - fixes TAP networking issues"
    @echo "  - Consistent environment across platforms"
    @echo "  - All ARM toolchain and Rust targets pre-installed"
