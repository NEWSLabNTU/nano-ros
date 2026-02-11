# Common clippy lints for real-time safety
CLIPPY_LINTS := "-D warnings -D clippy::infinite_iter -D clippy::while_immutable_condition -D clippy::never_loop -D clippy::empty_loop -D clippy::unconditional_recursion -W clippy::large_stack_arrays -W clippy::large_types_passed_by_value"

# Example lists (single source of truth for build/format/check/clean recipes)
NATIVE_EXAMPLES := "rs-talker rs-listener rs-custom-msg rs-service-server rs-service-client rs-action-server rs-action-client"
EMBEDDED_EXAMPLES := "stm32f4-rtic stm32f4-embassy stm32f4-polling stm32f4-smoltcp"
QEMU_EXAMPLES := "qemu/rs-test qemu/rs-wcet-bench"
QEMU_REFERENCE_EXAMPLES := "qemu-smoltcp-bridge qemu-lan9118"
QEMU_ZENOH_EXAMPLES := "qemu/rs-talker qemu/rs-listener qemu/bsp-talker qemu/bsp-listener"

LOG_DIR := "test-logs"

default:
    @just --list

# =============================================================================
# Entry Points
# =============================================================================

# Build everything: refresh bindings, workspace (native + embedded) and all examples
build: generate-bindings build-workspace build-workspace-embedded build-examples
    @echo "All builds completed!"

# Format everything: workspace and all examples
format: format-workspace format-examples
    @echo "All formatting completed!"

# Check everything: formatting, clippy (native + embedded + features), and all examples
check: check-workspace check-workspace-embedded check-workspace-features check-examples
    @echo "All checks passed!"

# Run unit tests only (no external dependencies)
test-unit verbose="":
    #!/usr/bin/env bash
    args=(--workspace --exclude nano-ros-tests --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run standard tests (needs qemu-system-arm + zenohd)
# Single nextest run (workspace + integration, excluding zephyr/ros2) + Miri + QEMU
test verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just _init-test-logs
    args=(--workspace --no-fail-fast
          -E 'not binary(zephyr) and not binary(rmw_interop)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}" || failed=1
    echo ""
    echo "=== Miri ==="
    just test-miri || failed=1
    echo ""
    echo "=== QEMU Tests ==="
    just test-qemu-basic {{verbose}} || failed=1
    just test-qemu-wcet {{verbose}} || failed=1
    just test-qemu-lan9118 {{verbose}} || failed=1
    echo ""
    echo "JUnit XML: target/nextest/default/junit.xml"
    echo "QEMU logs: {{LOG_DIR}}/latest/"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All standard tests passed!"
    fi

# Run all tests including Zephyr, ROS 2 interop, C API
# Single nextest run (entire workspace) + Miri + QEMU + C
test-all verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just _init-test-logs
    args=(--workspace --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}" || failed=1
    echo ""
    echo "=== Miri ==="
    just test-miri || failed=1
    echo ""
    echo "=== QEMU Tests ==="
    just test-qemu-basic {{verbose}} || failed=1
    just test-qemu-wcet {{verbose}} || failed=1
    just test-qemu-lan9118 {{verbose}} || failed=1
    echo ""
    echo "=== C API Tests ==="
    just test-c {{verbose}} || failed=1
    echo ""
    echo "JUnit XML:  target/nextest/default/junit.xml"
    echo "Other logs: {{LOG_DIR}}/latest/"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All tests passed!"
    fi

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
    just test-miri
    if [ $? -ne 0 ]; then
        echo "[FAIL] Miri FAILED"
        failed=1
    else
        echo "[OK] Miri passed"
    fi

    echo ""
    echo "=== QEMU Examples ==="
    qemu_failed=0
    for ex in {{QEMU_EXAMPLES}} {{QEMU_ZENOH_EXAMPLES}}; do
        (cd examples/$ex && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}) || qemu_failed=1
    done
    for ex in {{QEMU_REFERENCE_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}}) || qemu_failed=1
    done
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
# Test Infrastructure
# =============================================================================

# Initialize timestamped log directory for non-nextest test output (QEMU, C)
_init-test-logs:
    #!/usr/bin/env bash
    timestamp=$(date +%Y%m%d-%H%M%S)
    mkdir -p "{{LOG_DIR}}/$timestamp"
    ln -sfn "$timestamp" "{{LOG_DIR}}/latest"

# View JUnit XML test report (requires: npm install -g junit-cli-report-viewer)
test-report:
    @junit-cli-report-viewer target/nextest/default/junit.xml

# =============================================================================
# Workspace
# =============================================================================

# Build workspace (no_std, native)
# Excludes nano-ros-c which currently requires std
build-workspace:
    cargo build --workspace --no-default-features --exclude nano-ros-c
    cargo nextest run --workspace --no-run

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

# Run workspace unit tests (no external deps)
# Excludes nano-ros-tests which contains integration tests requiring zenohd/Zephyr/ROS 2
test-workspace verbose="":
    #!/usr/bin/env bash
    set -e
    args=(--workspace --exclude nano-ros-tests --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run Miri to detect undefined behavior in embedded-safe crates (no FFI)
test-miri:
    @echo "Running Miri on embedded-safe crates..."
    CARGO_PROFILE_DEV_OPT_LEVEL=0 cargo +nightly miri test -p nano-ros-serdes -p nano-ros-core -p nano-ros-params

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
    #!/usr/bin/env bash
    set -e
    echo "Building native examples..."
    for ex in {{NATIVE_EXAMPLES}}; do
        (cd examples/native/$ex && cargo build)
    done

# Format native examples
format-examples-native:
    #!/usr/bin/env bash
    set -e
    echo "Formatting native examples..."
    for ex in {{NATIVE_EXAMPLES}}; do
        (cd examples/native/$ex && cargo +nightly fmt)
    done

# Check native examples
check-examples-native:
    #!/usr/bin/env bash
    set -e
    echo "Checking native examples..."
    for ex in {{NATIVE_EXAMPLES}}; do
        (cd examples/native/$ex && cargo +nightly fmt --check && cargo clippy -- {{CLIPPY_LINTS}})
    done

# =============================================================================
# Examples - Embedded (STM32F4)
# =============================================================================

# Build embedded examples
build-examples-embedded:
    #!/usr/bin/env bash
    set -e
    echo "Building embedded examples..."
    for ex in {{EMBEDDED_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo build --release)
    done

# Format embedded examples
format-examples-embedded:
    #!/usr/bin/env bash
    set -e
    echo "Formatting embedded examples..."
    for ex in {{EMBEDDED_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo +nightly fmt)
    done

# Check embedded examples
check-examples-embedded:
    #!/usr/bin/env bash
    set -e
    echo "Checking embedded examples..."
    for ex in {{EMBEDDED_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}})
    done

# Show embedded example binary sizes
size-examples-embedded: build-examples-embedded
    @echo ""
    @echo "Binary sizes (release):"
    @echo "======================="
    @size packages/reference/stm32f4-rtic/target/thumbv7em-none-eabihf/release/stm32f4-rtic-example 2>/dev/null || echo "RTIC: build failed"
    @size packages/reference/stm32f4-embassy/target/thumbv7em-none-eabihf/release/stm32f4-embassy-example 2>/dev/null || echo "Embassy: build failed"
    @size packages/reference/stm32f4-polling/target/thumbv7em-none-eabihf/release/stm32f4-polling-example 2>/dev/null || echo "Polling: build failed"
    @size packages/reference/stm32f4-smoltcp/target/thumbv7em-none-eabihf/release/stm32f4-smoltcp 2>/dev/null || echo "stm32f4-smoltcp: build failed"

# Clean embedded example build artifacts
clean-examples-embedded:
    #!/usr/bin/env bash
    for ex in {{EMBEDDED_EXAMPLES}}; do
        rm -rf packages/reference/$ex/target
    done
    echo "Embedded example build artifacts cleaned"

# Clean native example build artifacts
clean-examples-native:
    #!/usr/bin/env bash
    for ex in {{NATIVE_EXAMPLES}}; do
        rm -rf examples/native/$ex/target
    done
    echo "Native example build artifacts cleaned"

# Clean QEMU example build artifacts
clean-examples-qemu:
    #!/usr/bin/env bash
    for ex in {{QEMU_EXAMPLES}} {{QEMU_ZENOH_EXAMPLES}}; do
        rm -rf examples/$ex/target
    done
    for ex in {{QEMU_REFERENCE_EXAMPLES}}; do
        rm -rf packages/reference/$ex/target
    done
    echo "QEMU example build artifacts cleaned"

# Clean all example build artifacts
clean-examples: clean-examples-native clean-examples-embedded clean-examples-qemu clean-examples-c
    @echo "All example build artifacts cleaned"

# =============================================================================
# Examples - Zephyr (native_sim)
# =============================================================================

# Zephyr workspace path (symlink or sibling directory)
ZEPHYR_WORKSPACE := if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else { "../nano-ros-workspace" }

# Build Zephyr Rust examples (all Rust examples for native_sim)
build-zephyr:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr Rust examples in $WORKSPACE..."
    cd "$WORKSPACE"
    echo "  Building zephyr/rs-talker -> build-talker/"
    west build -b native_sim/native/64 -d build-talker -p auto nano-ros/examples/zephyr/rs-talker
    echo "  Building zephyr/rs-listener -> build-listener/"
    west build -b native_sim/native/64 -d build-listener -p auto nano-ros/examples/zephyr/rs-listener
    echo "  Building zephyr/rs-service-server -> build-service-server/"
    west build -b native_sim/native/64 -d build-service-server -p auto nano-ros/examples/zephyr/rs-service-server
    echo "  Building zephyr/rs-service-client -> build-service-client/"
    west build -b native_sim/native/64 -d build-service-client -p auto nano-ros/examples/zephyr/rs-service-client
    echo "  Building zephyr/rs-action-server -> build-action-server/"
    west build -b native_sim/native/64 -d build-action-server -p auto nano-ros/examples/zephyr/rs-action-server
    echo "  Building zephyr/rs-action-client -> build-action-client/"
    west build -b native_sim/native/64 -d build-action-client -p auto nano-ros/examples/zephyr/rs-action-client
    echo "Zephyr Rust examples built successfully!"

# Build Zephyr C examples
build-zephyr-c:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr C examples in $WORKSPACE..."
    cd "$WORKSPACE"
    echo "  Building zephyr/c-talker -> build-c-talker/"
    west build -b native_sim/native/64 -d build-c-talker -p auto nano-ros/examples/zephyr/c-talker
    echo "  Building zephyr/c-listener -> build-c-listener/"
    west build -b native_sim/native/64 -d build-c-listener -p auto nano-ros/examples/zephyr/c-listener
    echo "Zephyr C examples built successfully!"

# Build all Zephyr examples (Rust + C)
build-zephyr-all: build-zephyr build-zephyr-c
    @echo "All Zephyr examples built!"

# Clean Zephyr build directories
clean-zephyr:
    #!/usr/bin/env bash
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    rm -rf "$WORKSPACE/build-talker" "$WORKSPACE/build-listener" "$WORKSPACE/build-service-server" "$WORKSPACE/build-service-client" "$WORKSPACE/build-action-server" "$WORKSPACE/build-action-client" "$WORKSPACE/build-c-talker" "$WORKSPACE/build-c-listener"
    echo "Zephyr build directories cleaned"

# Force rebuild Zephyr examples
rebuild-zephyr: clean-zephyr build-zephyr

# =============================================================================
# Examples - QEMU (Cortex-M3)
# =============================================================================

# Build QEMU examples (zenoh-pico is built inline by zenoh-pico-shim-sys)
build-examples-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Building QEMU examples..."
    for ex in {{QEMU_EXAMPLES}} {{QEMU_ZENOH_EXAMPLES}}; do
        (cd examples/$ex && cargo build --release)
    done
    for ex in {{QEMU_REFERENCE_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo build --release)
    done

# Format QEMU examples
format-examples-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Formatting QEMU examples..."
    for ex in {{QEMU_EXAMPLES}} {{QEMU_ZENOH_EXAMPLES}}; do
        (cd examples/$ex && cargo +nightly fmt)
    done
    for ex in {{QEMU_REFERENCE_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo +nightly fmt)
    done

# Check QEMU examples
check-examples-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Checking QEMU examples..."
    for ex in {{QEMU_EXAMPLES}} {{QEMU_ZENOH_EXAMPLES}}; do
        (cd examples/$ex && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}})
    done
    for ex in {{QEMU_REFERENCE_EXAMPLES}}; do
        (cd packages/reference/$ex && cargo +nightly fmt --check && cargo clippy --release -- {{CLIPPY_LINTS}})
    done

# Run all QEMU tests (non-networked)
test-qemu verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just _init-test-logs
    just test-qemu-basic {{verbose}} || failed=1
    just test-qemu-wcet {{verbose}} || failed=1
    just test-qemu-lan9118 {{verbose}} || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some QEMU tests failed."
        exit 1
    else
        echo "All QEMU tests passed!"
    fi

# Build zenoh-pico for ARM Cortex-M3 (required for QEMU examples)
build-zenoh-pico-arm:
    @./scripts/qemu/build-zenoh-pico.sh

# Clean zenoh-pico ARM build
clean-zenoh-pico-arm:
    ./scripts/qemu/build-zenoh-pico.sh --clean

# Build zenoh-pico for ESP32-C3 (RISC-V RV32IMC)
build-zenoh-pico-riscv:
    @./scripts/esp32/build-zenoh-pico.sh

# Clean zenoh-pico RISC-V build
clean-zenoh-pico-riscv:
    ./scripts/esp32/build-zenoh-pico.sh --clean

# Build ESP32 examples (requires nightly; zenoh-pico is built inline)
build-examples-esp32:
    #!/usr/bin/env bash
    set -e
    echo "Building ESP32 examples..."
    for ex in bsp-talker bsp-listener; do
        echo "  Building esp32/$ex..."
        (cd examples/esp32/$ex && SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" cargo +nightly build --release)
    done
    echo "ESP32 examples built!"

# Build ESP32 QEMU examples (requires nightly; zenoh-pico is built inline)
build-examples-esp32-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Building ESP32 QEMU examples..."
    for ex in qemu-talker qemu-listener; do
        echo "  Building esp32/$ex..."
        (cd examples/esp32/$ex && cargo +nightly build --release)
    done
    echo ""
    echo "Creating flash images..."
    mkdir -p build/esp32-qemu
    for ex in qemu-talker qemu-listener; do
        bin_name="esp32-$ex"
        elf="examples/esp32/$ex/target/riscv32imc-unknown-none-elf/release/$bin_name"
        out="build/esp32-qemu/$bin_name.bin"
        if command -v espflash &>/dev/null; then
            espflash save-image --chip esp32c3 --flash-size 4mb --merge "$elf" "$out"
            echo "  $out"
        else
            echo "  WARNING: espflash not found, skipping flash image for $ex"
            echo "  Install with: cargo install espflash"
        fi
    done
    echo "ESP32 QEMU examples built!"

# Run basic QEMU ESP32-C3 boot test (verify UART output)
test-qemu-esp32-basic: build-examples-esp32-qemu
    #!/usr/bin/env bash
    echo "ESP32-C3 QEMU boot test"
    echo "========================"
    echo ""
    if ! command -v qemu-system-riscv32 &>/dev/null; then
        echo "WARNING: qemu-system-riscv32 not found - skipping runtime test"
        echo "Flash images are at: build/esp32-qemu/"
        exit 0
    fi
    echo "Running boot test..."
    tmpfile=$(mktemp)
    trap 'rm -f "$tmpfile"' EXIT
    timeout 20 qemu-system-riscv32 -M esp32c3 -icount 3 -nographic \
        -drive "file=build/esp32-qemu/esp32-qemu-talker.bin,if=mtd,format=raw" \
        -nic none \
        > "$tmpfile" 2>&1 || true
    cat "$tmpfile"
    echo ""
    if grep -q "nano-ros ESP32-C3 QEMU BSP" "$tmpfile"; then
        echo "[PASS] ESP32-C3 QEMU boot test - BSP initialized"
    else
        echo "[FAIL] ESP32-C3 QEMU boot test - BSP banner not found"
        exit 1
    fi

# Run ESP32-C3 QEMU integration tests (build, boot, E2E via nextest)
test-qemu-esp32 verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nano-ros-tests --test esp32_emulator --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run basic QEMU test (nano-ros serialization on Cortex-M3)
test-qemu-basic verbose="": build-examples-qemu _init-test-logs
    ./tests/run-test.sh --name qemu-basic --log {{LOG_DIR}}/latest/qemu-basic.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu/rs-test/target/thumbv7m-none-eabi/release/qemu-rs-test

# Run WCET benchmark on QEMU (DWT cycle counter)
test-qemu-wcet verbose="": build-examples-qemu _init-test-logs
    ./tests/run-test.sh --name qemu-wcet-bench --log {{LOG_DIR}}/latest/qemu-wcet-bench.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu/rs-wcet-bench/target/thumbv7m-none-eabi/release/qemu-rs-wcet-bench

# Run LAN9118 Ethernet driver test (mps2-an385)
test-qemu-lan9118 verbose="": build-examples-qemu _init-test-logs
    ./tests/run-test.sh --name qemu-lan9118 --log {{LOG_DIR}}/latest/qemu-lan9118.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
            -semihosting-config enable=on,target=native \
            -kernel packages/reference/qemu-lan9118/target/thumbv7m-none-eabi/release/qemu-rs-lan9118

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
test-qemu-zenoh: build-examples-qemu
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
    echo "  Terminal 2: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary examples/qemu/rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker"
    echo "  Terminal 3: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 --binary examples/qemu/rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener"
    echo ""
    echo "Binaries built at:"
    echo "  examples/qemu/rs-talker/target/thumbv7m-none-eabi/release/qemu-rs-talker"
    echo "  examples/qemu/rs-listener/target/thumbv7m-none-eabi/release/qemu-rs-listener"
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
    @echo "  just test-qemu-wcet          # Run WCET benchmark"
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

# =============================================================================
# Static Analysis
# =============================================================================

# Inspect generated assembly for a function (requires cargo-show-asm)
# Usage: just show-asm <package> <function> [target]
# Examples:
#   just show-asm nano-ros-serdes 'CdrWriter::write_string'
#   just show-asm nano-ros-serdes 'CdrWriter::write_string' thumbv7m-none-eabi
#   just show-asm nano-ros-core 'Duration::from_nanos'
show-asm pkg fn target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib "{{fn}}" --rust)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}"

# Show llvm-mca throughput analysis for a function (requires cargo-show-asm)
# Usage: just show-asm-mca <package> <function> [target]
show-asm-mca pkg fn target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib "{{fn}}" --mca)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}"

# List all non-inlined functions in a crate (useful for finding inspectable symbols)
# Usage: just show-asm-list <package> [target]
show-asm-list pkg target="":
    #!/usr/bin/env bash
    set -euo pipefail
    args=(-p "{{pkg}}" --lib)
    if [[ -n "{{target}}" ]]; then
        args+=(--target "{{target}}" --no-default-features)
    fi
    cargo asm "${args[@]}" || true

# Analyze per-function stack usage (requires nightly + llvm-tools)
# Usage: just check-stack [example-dir] [top]
# Default: examples/qemu/rs-wcet-bench, top 30
check-stack example="examples/qemu/rs-wcet-bench" top="30":
    ./scripts/stack-analysis.sh {{example}} --top {{top}}

# Analyze stack usage of a pre-built ELF (e.g. Zephyr west build output)
# Usage: just check-stack-elf <path-to-elf> [top]
check-stack-elf elf top="30":
    ./scripts/stack-analysis.sh --elf {{elf}} --top {{top}}

# Analyze stack usage of C examples (requires cmake + gcc)
# Usage: just check-stack-c [example-dir] [top]
# Default: examples/native/c-talker, top 30
check-stack-c example="examples/native/c-talker" top="30":
    ./scripts/stack-analysis-c.sh {{example}} --top {{top}}

# Analyze stack usage of all examples (requires nightly + llvm-tools + cmake)
# Covers: QEMU ARM, native Rust, and native C examples
# ESP32/STM32F4 excluded (need platform-specific SDKs)
check-stack-all top="10":
    #!/usr/bin/env bash
    set -euo pipefail
    failed=0
    # Rust examples (QEMU ARM — no exclude, show full picture)
    for example in \
        examples/qemu/rs-wcet-bench \
        examples/qemu/rs-test \
        examples/qemu/rs-talker \
        examples/qemu/rs-listener \
        examples/qemu/bsp-talker \
        examples/qemu/bsp-listener \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # Rust examples (native — exclude tracing/regex infrastructure noise)
    for example in \
        examples/native/rs-talker \
        examples/native/rs-listener \
        examples/native/rs-custom-msg \
        examples/native/rs-service-server \
        examples/native/rs-service-client \
        examples/native/rs-action-server \
        examples/native/rs-action-client \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} --exclude "regex_automata|regex_syntax|aho_corasick|env_filter|env_logger|driftsort" || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # C examples (native)
    for example in \
        examples/native/c-talker \
        examples/native/c-listener \
        examples/native/c-custom-msg \
        examples/native/c-baremetal-demo \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis-c.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    if [ "$failed" -gt 0 ]; then
        echo "[WARN] $failed example(s) failed"
        exit 1
    fi
    echo "[OK] All stack analyses complete"

# Run Kani bounded model checking on core crates (requires kani-verifier)
# Proves panic-freedom, roundtrip correctness, and bounded behavior
verify-kani:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Kani Verification ==="
    failed=0
    for crate in nano-ros-serdes nano-ros-core nano-ros-params nano-ros-c; do
        echo ""
        echo "--- Verifying $crate ---"
        cargo kani -p "$crate" || { echo "[FAIL] $crate"; failed=$((failed + 1)); }
    done
    echo ""
    if [ "$failed" -gt 0 ]; then
        echo "[FAIL] $failed crate(s) failed verification"
        exit 1
    fi
    echo "[OK] All Kani proofs verified"

# =============================================================================
# Zenoh
# =============================================================================

# Build zenoh transport
build-zenoh:
    cargo build -p nano-ros-transport --features zenoh,std

# Check zenoh transport
check-zenoh:
    cargo clippy -p nano-ros-transport --features zenoh,std -- {{CLIPPY_LINTS}}

# Build zenohd 1.6.2 from submodule (version-matched to rmw_zenoh_cpp)
build-zenohd:
    ./scripts/zenohd/build.sh

# Clean zenohd build
clean-zenohd:
    ./scripts/zenohd/build.sh --clean

# Build zenoh-pico C library (standalone, for debugging)
build-zenoh-pico:
    @echo "Building zenoh-pico..."
    cd packages/transport/zenoh-pico-shim-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: packages/transport/zenoh-pico-shim-sys/zenoh-pico/build"

# =============================================================================
# Integration Tests (requires zenohd running on tcp/127.0.0.1:7447)
# =============================================================================

# Run all Rust integration tests (requires zenohd)
# Excludes zephyr and rmw_interop tests (run via test-zephyr / test-ros2)
test-integration verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nano-ros-tests --no-fail-fast
          -E 'not binary(zephyr) and not binary(rmw_interop) and not binary(esp32_emulator)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# =============================================================================
# Zephyr Tests (requires west workspace + bridge network)
# =============================================================================

# Run Zephyr E2E tests (requires pre-built Zephyr examples + bridge network)
# Note: thread limit handled by [test-groups.zephyr] in .config/nextest.toml
test-zephyr verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nano-ros-tests --test zephyr --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

# Run Zephyr tests with full rebuild
test-zephyr-full verbose="": build-zephyr
    just test-zephyr {{verbose}}

# Run Zephyr C examples test
test-zephyr-c:
    ./tests/zephyr/run-c.sh

# =============================================================================
# ROS 2 Interop Tests (requires ROS 2 + rmw_zenoh_cpp + zenohd)
# =============================================================================

# Run ROS 2 interop tests (Rust test harness)
test-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nano-ros-tests --test rmw_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run ROS 2 interop tests (shell script)
test-ros2-shell:
    ./tests/ros2-interop.sh

# =============================================================================
# C API Tests (requires cmake + zenohd)
# =============================================================================

# Run all C tests (integration + codegen)
test-c verbose="": _init-test-logs
    #!/usr/bin/env bash
    set -e
    v="{{ if verbose != "" { "--verbose" } else { "" } }}"
    # C API integration tests (build + communication via nextest)
    args=(-p nano-ros-tests --no-fail-fast -E 'binary(c_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    # C codegen tests
    ./tests/run-test.sh --name c-codegen --log {{LOG_DIR}}/latest/c-codegen.log $v -- \
        bash -c 'cd packages/codegen/packages && cargo test -p cargo-nano-ros --test test_generate_c -- --nocapture'
    ./tests/run-test.sh --name c-msg-gen --log {{LOG_DIR}}/latest/c-msg-gen.log $v -- ./tests/c-msg-gen-tests.sh

# Build C examples only (no tests)
build-examples-c: build-codegen-lib
    @echo "Building nano-ros-c library..."
    cargo build -p nano-ros-c --release
    @echo "Building native/c-talker..."
    cd examples/native/c-talker && rm -rf build && mkdir -p build && cd build && cmake -DNANO_ROS_ROOT="$(cd ../../../.. && pwd)" .. && make
    @echo "Building native/c-listener..."
    cd examples/native/c-listener && rm -rf build && mkdir -p build && cd build && cmake -DNANO_ROS_ROOT="$(cd ../../../.. && pwd)" .. && make
    @echo "Building native/c-custom-msg..."
    cd examples/native/c-custom-msg && rm -rf build && mkdir -p build && cd build && cmake -DNANO_ROS_ROOT="$(cd ../../../.. && pwd)" .. && make
    @echo "C examples built!"

# Clean C examples build
clean-examples-c:
    rm -rf examples/native/c-talker/build examples/native/c-listener/build examples/native/c-custom-msg/build
    @echo "C examples build cleaned"

# =============================================================================
# Message Bindings
# =============================================================================

# Build the codegen static library (for CMake C code generation)
build-codegen-lib:
    @echo "Building nano-ros-codegen-c staticlib..."
    cargo build -p nano-ros-codegen-c --release --manifest-path packages/codegen/packages/Cargo.toml

# Install cargo-nano-ros (requires ROS 2 environment)
install-cargo-nano-ros:
    @echo "Installing cargo-nano-ros..."
    cargo install --path packages/codegen/packages/cargo-nano-ros --locked

# Regenerate Rust bindings in all examples and rcl-interfaces
# Uses bundled interfaces (std_msgs, builtin_interfaces) — no ROS 2 environment required
generate-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Building nano-ros codegen tool..."
    cargo build --manifest-path packages/codegen/packages/Cargo.toml -p cargo-nano-ros --bin nano-ros
    NANO_ROS="$(pwd)/packages/codegen/packages/target/debug/nano-ros"
    echo "Regenerating Rust bindings..."

    # Internal crate (workspace member — checked into git)
    echo "  rcl-interfaces"
    (cd packages/interfaces/rcl-interfaces && $NANO_ROS generate-rust)

    # Native examples
    for ex in rs-talker rs-listener rs-custom-msg rs-service-server rs-service-client rs-action-server rs-action-client; do
        echo "  native/$ex"
        (cd examples/native/$ex && $NANO_ROS generate-rust)
    done

    # QEMU examples
    for ex in rs-test rs-wcet-bench rs-talker rs-listener bsp-talker bsp-listener; do
        echo "  qemu/$ex"
        (cd examples/qemu/$ex && $NANO_ROS generate-rust)
    done

    # ESP32 WiFi examples
    for ex in bsp-talker bsp-listener; do
        echo "  esp32/$ex"
        (cd examples/esp32/$ex && $NANO_ROS generate-rust)
    done

    # STM32F4 examples
    echo "  stm32f4/bsp-talker"
    (cd examples/stm32f4/bsp-talker && $NANO_ROS generate-rust)

    # Zephyr examples
    for ex in rs-talker rs-listener rs-service-server rs-service-client rs-action-server rs-action-client; do
        echo "  zephyr/$ex"
        (cd examples/zephyr/$ex && $NANO_ROS generate-rust)
    done

    echo "All bindings regenerated!"

# Remove generated/ directories in examples (not rcl-interfaces — it's a workspace member)
clean-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Removing generated bindings..."
    dirs=(
        examples/native/rs-talker/generated
        examples/native/rs-listener/generated
        examples/native/rs-custom-msg/generated
        examples/native/rs-service-server/generated
        examples/native/rs-service-client/generated
        examples/native/rs-action-server/generated
        examples/native/rs-action-client/generated
        examples/qemu/rs-test/generated
        examples/qemu/rs-wcet-bench/generated
        examples/qemu/rs-talker/generated
        examples/qemu/rs-listener/generated
        examples/qemu/bsp-talker/generated
        examples/qemu/bsp-listener/generated
        examples/esp32/bsp-talker/generated
        examples/esp32/bsp-listener/generated
        examples/stm32f4/bsp-talker/generated
        examples/zephyr/rs-talker/generated
        examples/zephyr/rs-listener/generated
        examples/zephyr/rs-service-server/generated
        examples/zephyr/rs-service-client/generated
        examples/zephyr/rs-action-server/generated
        examples/zephyr/rs-action-client/generated
    )
    for d in "${dirs[@]}"; do
        if [ -d "$d" ]; then
            rm -rf "$d"
            echo "  removed $d"
        fi
    done
    echo "All generated bindings removed."

# Clean and regenerate all bindings from scratch
regenerate-bindings: clean-bindings generate-bindings

# =============================================================================
# Setup & Cleanup
# =============================================================================

# Install toolchains and tools (interactive — lists actions and asks for confirmation)
setup:
    #!/usr/bin/env bash
    set -e
    echo "=== nano-ros setup ==="
    echo ""
    echo "This will:"
    echo "  1. Install system packages via apt (may prompt for sudo):"
    echo "       gcc-arm-none-eabi, qemu-system-arm, cmake,"
    echo "       gcc-riscv64-unknown-elf, picolibc-riscv64-unknown-elf"
    echo "  2. Install Rust toolchains (stable + nightly)"
    echo "  3. Add rustup components: rustfmt, clippy, rust-src, miri"
    echo "  4. Add cross-compilation targets:"
    echo "       - thumbv7em-none-eabihf  (ARM Cortex-M4F)"
    echo "       - thumbv7m-none-eabi     (ARM Cortex-M3)"
    echo "       - riscv32imc-unknown-none-elf (ESP32-C3 RISC-V)"
    echo "  5. Install cargo tools:"
    echo "       - cargo-nextest          (test runner)"
    echo "       - espflash               (ESP32 flash tool)"
    echo "       - cargo-nano-ros         (message binding generator)"
    echo "  6. Build Espressif QEMU from source → ~/.local/bin/qemu-system-riscv32"
    echo "     (ESP32-C3 emulator — requires git, ninja, python3, pkg-config,"
    echo "      libglib2.0-dev, libpixman-1-dev, libgcrypt20-dev, libslirp-dev)"
    echo ""
    read -r -p "Proceed? [Y/n] " answer
    if [[ "$answer" =~ ^[Nn] ]]; then
        echo "Setup cancelled."
        exit 0
    fi
    echo ""

    echo "=== [1/6] System packages (apt) ==="
    apt_pkgs=()
    check_apt() {
        if command -v "$2" &>/dev/null; then
            printf "  %-40s %s\n" "$1" "[already installed]"
        else
            apt_pkgs+=("$1")
        fi
    }
    check_apt gcc-arm-none-eabi              arm-none-eabi-gcc
    check_apt qemu-system-arm                qemu-system-arm
    check_apt cmake                          cmake
    check_apt gcc-riscv64-unknown-elf        riscv64-unknown-elf-gcc
    # picolibc: check via sysroot or known path (no binary to test)
    picolibc_found=false
    if command -v riscv64-unknown-elf-gcc &>/dev/null; then
        sysroot=$(riscv64-unknown-elf-gcc -march=rv32imc -mabi=ilp32 --specs=picolibc.specs -print-sysroot 2>/dev/null || true)
        if [ -n "$sysroot" ] && [ -d "$sysroot/include" ]; then
            picolibc_found=true
        elif [ -d "/usr/lib/picolibc/riscv64-unknown-elf/include" ]; then
            picolibc_found=true
        fi
    fi
    if $picolibc_found; then
        printf "  %-40s %s\n" "picolibc-riscv64-unknown-elf" "[already installed]"
    else
        apt_pkgs+=("picolibc-riscv64-unknown-elf")
    fi
    if [ ${#apt_pkgs[@]} -gt 0 ]; then
        echo ""
        echo "  Installing: ${apt_pkgs[*]}"
        sudo apt-get install -y "${apt_pkgs[@]}"
    else
        echo "  All system packages already installed."
    fi
    echo ""

    echo "=== [2/6] Installing Rust toolchains ==="
    rustup toolchain install stable
    rustup toolchain install nightly
    echo ""

    echo "=== [3/6] Adding rustup components ==="
    rustup component add rustfmt clippy rust-src
    rustup component add --toolchain nightly rustfmt miri rust-src llvm-tools
    echo ""

    echo "=== [4/6] Adding cross-compilation targets ==="
    rustup target add thumbv7em-none-eabihf
    rustup target add thumbv7m-none-eabi
    rustup target add riscv32imc-unknown-none-elf
    rustup +nightly target add thumbv7m-none-eabi
    echo ""

    echo "=== [5/6] Installing cargo tools ==="
    cargo install cargo-nextest --locked
    cargo install espflash --locked || echo "WARNING: espflash install failed (non-fatal)"
    cargo install rustfilt --locked || echo "WARNING: rustfilt install failed (non-fatal)"
    cargo install cargo-show-asm --locked || echo "WARNING: cargo-show-asm install failed (non-fatal)"
    cargo install --locked kani-verifier && cargo kani setup || echo "WARNING: kani install failed (non-fatal)"
    cargo install --path packages/codegen/packages/cargo-nano-ros --locked
    echo ""

    echo "=== [6/6] Building Espressif QEMU (qemu-system-riscv32) ==="
    if command -v qemu-system-riscv32 &>/dev/null; then
        echo "Already installed: $(qemu-system-riscv32 --version | head -1)"
        echo "Skipping build. To reinstall, run: ./scripts/esp32/install-espressif-qemu.sh"
    else
        ./scripts/esp32/install-espressif-qemu.sh
    fi
    echo ""
    echo "Setup complete!"

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
clean: clean-examples clean-zephyr clean-zenohd
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
    @echo "  just build-zephyr           # Build all Rust Zephyr examples"
    @echo "  just build-zephyr-c         # Build C Zephyr examples"
    @echo "  just build-zephyr-all       # Build all Zephyr examples (Rust + C)"
    @echo "  just rebuild-zephyr         # Clean and rebuild"
    @echo "  just clean-zephyr           # Remove all build directories"
    @echo ""
    @echo "Run tests:"
    @echo "  just test-zephyr            # Run all Zephyr E2E tests"
    @echo "  just test-zephyr-full       # Rebuild and run all Zephyr tests"
    @echo "  just test-zephyr-c          # Run Zephyr C examples test"
    @echo ""
    @echo "Manual build (from Zephyr workspace):"
    @echo "  west build -b native_sim/native/64 -d build-talker nano-ros/examples/zephyr/rs-talker"
    @echo "  west build -b native_sim/native/64 -d build-listener nano-ros/examples/zephyr/rs-listener"
    @echo "  west build -b native_sim/native/64 -d build-action-server nano-ros/examples/zephyr/rs-action-server"
    @echo "  west build -b native_sim/native/64 -d build-action-client nano-ros/examples/zephyr/rs-action-client"

# =============================================================================
# Docker
# =============================================================================

# Build Docker image for QEMU ARM development
docker-build:
    @echo "Building Docker image..."
    docker build -t nano-ros-qemu -f tests/qemu-baremetal/Dockerfile .
    @echo "Docker image 'nano-ros-qemu' built successfully!"

# Start Docker container with interactive shell
docker-shell:
    @echo "Starting Docker container..."
    docker run -it --rm \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nano-ros-cargo-registry:/cargo/registry \
        -v nano-ros-cargo-git:/cargo/git \
        nano-ros-qemu

# Start Docker container with TAP networking support
docker-shell-network:
    @echo "Starting Docker container with networking support..."
    docker run -it --rm \
        --cap-add=NET_ADMIN \
        --device=/dev/net/tun \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nano-ros-cargo-registry:/cargo/registry \
        -v nano-ros-cargo-git:/cargo/git \
        nano-ros-qemu

# Run bare-metal QEMU talker/listener test using Docker Compose (rs-* examples)
# Uses separate containers for zenohd, talker, and listener with isolated networking
# Each container creates its own TAP/bridge and NATs to the Docker network
test-docker-qemu: docker-build build-examples-qemu
    @echo "Running bare-metal QEMU talker/listener test (rs-* examples)..."
    @echo "This starts 3 containers: zenohd, talker, and listener"
    @echo ""
    HOST_UID=$(id -u) HOST_GID=$(id -g) QEMU_EXAMPLE=rs docker compose -f tests/qemu-baremetal/docker-compose.yml up --build --abort-on-container-exit
    @docker compose -f tests/qemu-baremetal/docker-compose.yml down -v 2>/dev/null || true

# Run bare-metal QEMU talker/listener test using BSP examples
test-docker-qemu-bsp: docker-build build-examples-qemu
    @echo "Running bare-metal QEMU talker/listener test (bsp-* examples)..."
    @echo "This starts 3 containers: zenohd, talker, and listener"
    @echo ""
    HOST_UID=$(id -u) HOST_GID=$(id -g) QEMU_EXAMPLE=bsp docker compose -f tests/qemu-baremetal/docker-compose.yml up --build --abort-on-container-exit
    @docker compose -f tests/qemu-baremetal/docker-compose.yml down -v 2>/dev/null || true

# Run QEMU build inside Docker container
docker-build-qemu: docker-build
    @echo "Building QEMU examples inside Docker..."
    docker run --rm \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nano-ros-cargo-registry:/cargo/registry \
        -v nano-ros-cargo-git:/cargo/git \
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
    @echo "  just docker-build           # Build nano-ros-qemu image"
    @echo ""
    @echo "Interactive shell:"
    @echo "  just docker-shell           # Start container with shell"
    @echo "  just docker-shell-network   # Start with TAP networking support"
    @echo ""
    @echo "Run commands in Docker:"
    @echo "  just test-docker-qemu       # Run QEMU talker/listener test (rs-* examples)"
    @echo "  just test-docker-qemu-bsp   # Run QEMU talker/listener test (bsp-* examples)"
    @echo "  just docker-build-qemu      # Build QEMU examples in Docker"
    @echo ""
    @echo "Docker Compose:"
    @echo "  just docker-up              # Start development services"
    @echo "  just docker-down            # Stop development services"
    @echo "  just docker-exec            # Execute command in container"
    @echo ""
    @echo "Benefits:"
    @echo "  - QEMU 7.2 (Debian bookworm) - fixes TAP networking issues"
    @echo "  - Consistent environment across platforms"
    @echo "  - All ARM toolchain and Rust targets pre-installed"
