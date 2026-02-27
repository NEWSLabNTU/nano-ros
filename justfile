# Common clippy lints for real-time safety
CLIPPY_LINTS := "-D warnings -D clippy::infinite_iter -D clippy::while_immutable_condition -D clippy::never_loop -D clippy::empty_loop -D clippy::unconditional_recursion -W clippy::large_stack_arrays -W clippy::large_types_passed_by_value"

LOG_DIR := "test-logs"

# Default paths for external SDKs — exported so all recipes (build + test) see them
export FREERTOS_DIR := env("FREERTOS_DIR", justfile_directory() / "external/freertos-kernel")
export FREERTOS_PORT := env("FREERTOS_PORT", "GCC/ARM_CM3")
export LWIP_DIR := env("LWIP_DIR", justfile_directory() / "external/lwip")
export FREERTOS_CONFIG_DIR := env("FREERTOS_CONFIG_DIR", justfile_directory() / "packages/boards/nros-mps2-an385-freertos/config")
export NUTTX_DIR := env("NUTTX_DIR", justfile_directory() / "external/nuttx")
export THREADX_DIR := env("THREADX_DIR", justfile_directory() / "external/threadx")
export THREADX_CONFIG_DIR := env("THREADX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-threadx-linux/config")
export NETX_DIR := env("NETX_DIR", justfile_directory() / "external/netxduo")
export NETX_CONFIG_DIR := env("NETX_CONFIG_DIR", justfile_directory() / "packages/boards/nros-threadx-linux/config")

default:
    @just --list

# =============================================================================
# Entry Points
# =============================================================================

# Build everything: refresh bindings, workspace (native + embedded) and all examples
build: install-local generate-bindings build-workspace build-workspace-embedded build-examples
    @echo "All builds completed!"

# Populate build/install/ with C API artifacts (libraries, headers, CMake, codegen, interfaces).
# Builds both zenoh and XRCE RMW variants via CMake + Corrosion.
install-local:
    #!/usr/bin/env bash
    set -e
    PREFIX="$(pwd)/build/install"
    for rmw in zenoh xrce; do
        echo "=== Building RMW=$rmw ==="
        cmake -S . -B "build/cmake-$rmw" \
            -DNANO_ROS_RMW="$rmw" \
            -DCMAKE_BUILD_TYPE=Release
        cmake --build "build/cmake-$rmw"
        cmake --install "build/cmake-$rmw" --prefix "$PREFIX"
    done
    echo "Installed to $PREFIX"

# Format everything: workspace and all examples (parallel)
format:
    #!/usr/bin/env bash
    set -e
    {
        echo "."
        find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' \
            -not -path '*/generated/*' -not -path '*/zephyr/*' \
            -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
            -not -path '*/threadx-linux/*' \
            -exec dirname {} \; | sort
    } | parallel --halt now,fail=1 --line-buffer \
        'cd {} && cargo +nightly fmt && echo "  fmt {}"'
    echo "All formatting completed!"

# Check everything: formatting, clippy (native + embedded + features), and all examples
check: check-workspace check-workspace-embedded check-workspace-features check-examples
    @echo "All checks passed!"

# Run unit tests only (no external dependencies)
test-unit verbose="":
    #!/usr/bin/env bash
    args=(--workspace --exclude nros-tests --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run standard tests (needs qemu-system-arm + zenohd)
# Single nextest run (workspace + integration, excluding zephyr/ros2/large_msg) + Miri
test verbose="": build-zenohd
    #!/usr/bin/env bash
    set +e
    failed=0
    args=(--workspace --no-fail-fast
          -E 'not binary(zephyr) and not binary(rmw_interop) and not binary(xrce_ros2_interop) and not binary(esp32_emulator) and not binary(large_msg) and not binary(nuttx_qemu) and not binary(threadx_linux)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}" || failed=1
    echo ""
    echo "=== Miri ==="
    just test-miri || failed=1
    echo ""
    echo "JUnit XML: target/nextest/default/junit.xml"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All standard tests passed!"
    fi

# Run all tests including Zephyr, ROS 2 interop, C API, XRCE, NuttX, FreeRTOS, large_msg
# Single nextest run (entire workspace) + Miri + C codegen
test-all verbose="": build-zenohd
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
    echo "=== C Codegen Tests ==="
    just _test-c-codegen {{verbose}} || failed=1
    echo ""
    echo "JUnit XML:  target/nextest/default/junit.xml"
    echo "Other logs: {{LOG_DIR}}/latest/"
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some tests failed."
        exit 1
    else
        echo "All tests passed!"
    fi

# Run code quality checks: format check + clippy + tests (never modifies code)
quality: check test
    @echo "All quality checks passed!"

# Run full CI suite (quality + all integration tests)
ci: check test
    @echo "Full CI suite passed!"

# =============================================================================
# Test Infrastructure
# =============================================================================

# Kill orphaned test processes from previous runs
test-kill-orphans:
    #!/usr/bin/env bash
    echo "Killing orphaned test processes..."
    pkill -9 -f 'zenohd.*--listen.*--no-multicast' 2>/dev/null || true
    pkill -9 -f 'nano-ros/examples/.*/target/' 2>/dev/null || true
    pkill -9 -f 'nano-ros/examples/.*/build/' 2>/dev/null || true
    pkill -9 -f 'MicroXRCEAgent' 2>/dev/null || true
    pkill -9 -f 'ros2 topic' 2>/dev/null || true
    pkill -9 -f 'ros2 service' 2>/dev/null || true
    pkill -9 -f 'ros2 action' 2>/dev/null || true
    echo "Done."

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
# nros-c excluded from no_std build: staticlib/cdylib requires panic handler (needs std)
build-workspace:
    cargo build --workspace --no-default-features --exclude nros-c
    cargo nextest run --workspace --no-run

# Build workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c: staticlib/cdylib requires panic handler (needs std)
build-workspace-embedded:
    cargo build --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c

# Format workspace code
format-workspace:
    cargo +nightly fmt

# Check workspace: formatting and clippy (no_std, native)
# nros-c excluded from no_std check: staticlib/cdylib requires panic handler (needs std)
check-workspace:
    cargo +nightly fmt --check
    cargo clippy --workspace --no-default-features --exclude nros-c -- {{CLIPPY_LINTS}}

# Check workspace for embedded target (Cortex-M4F)
# Excludes zpico-sys: requires native system headers for CMake build
# Excludes nros-tests: requires std (test framework dependencies)
# Excludes nros-c: staticlib/cdylib requires panic handler (needs std)
check-workspace-embedded:
    @echo "Checking workspace for embedded target..."
    cargo clippy --workspace --no-default-features --target thumbv7em-none-eabihf \
        --exclude zpico-sys \
        --exclude nros-tests \
        --exclude nros-c -- {{CLIPPY_LINTS}}

# Check workspace with various feature combinations
check-workspace-features:
    @echo "Checking feature combinations..."
    @echo "  - nros: zenoh + posix + humble"
    cargo clippy -p nros --no-default-features --features "std,rmw-zenoh,platform-posix,ros-humble" -- {{CLIPPY_LINTS}}
    @echo "  - nros: zenoh + posix + iron"
    cargo clippy -p nros --no-default-features --features "std,rmw-zenoh,platform-posix,ros-iron" -- {{CLIPPY_LINTS}}
    @echo "  - nros-c: zenoh + posix + humble"
    cargo clippy -p nros-c --no-default-features --features "std,rmw-zenoh,platform-posix,ros-humble" -- {{CLIPPY_LINTS}}
    @echo "  - nros: cffi (no_std)"
    cargo clippy -p nros --no-default-features --features "rmw-cffi" -- {{CLIPPY_LINTS}}
    @echo "  - transport: sync-critical-section"
    cargo clippy -p nros-rmw --no-default-features --features "sync-critical-section" --target thumbv7em-none-eabihf -- {{CLIPPY_LINTS}}
    @echo "  - zenoh transport (std)"
    cargo clippy -p nros-rmw --features "std" -- {{CLIPPY_LINTS}}
    @echo "All feature checks passed!"

# Alias for test-unit (backward compatibility)
test-workspace verbose="": (test-unit verbose)

# Run Miri to detect undefined behavior in embedded-safe crates (no FFI)
test-miri:
    @echo "Running Miri on embedded-safe crates..."
    CARGO_PROFILE_DEV_OPT_LEVEL=0 cargo +nightly miri test -p nros-serdes -p nros-core -p nros-params

# =============================================================================
# Examples (auto-discovered from examples/{platform}/{lang}/{rmw}/{use-case}/)
# =============================================================================

# Build all Rust examples (auto-discovered, excludes zephyr which uses west)
build-examples:
    #!/usr/bin/env bash
    set -e
    echo "Building examples..."
    for toml in $(find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' -not -path '*/generated/*' -not -path '*/zephyr/*' -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' -not -path '*/threadx-linux/*' | sort); do
        dir="$(dirname "$toml")"
        platform="$(echo "$dir" | cut -d/ -f2)"
        flags=""
        env_prefix=""
        toolchain=""
        if [ "$platform" != "native" ]; then flags="--release"; fi
        # ESP32 WiFi examples need SSID/PASSWORD env vars and nightly toolchain
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
            toolchain="+nightly"
        fi
        echo "  build $dir"
        (cd "$dir" && eval $env_prefix cargo $toolchain build $flags)
    done
    echo "All examples built!"

# Format all Rust examples (auto-discovered, parallel)
format-examples:
    #!/usr/bin/env bash
    set -e
    echo "Formatting examples..."
    find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' \
        -not -path '*/generated/*' -not -path '*/zephyr/*' \
        -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
        -not -path '*/threadx-linux/*' \
        -exec dirname {} \; | sort \
    | parallel --halt now,fail=1 --line-buffer \
        'cd {} && cargo +nightly fmt && echo "  fmt {}"'
    echo "All examples formatted!"

# Check all Rust examples (auto-discovered, excludes zephyr which uses west)
check-examples:
    #!/usr/bin/env bash
    set -e
    echo "Checking examples..."
    for toml in $(find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' -not -path '*/generated/*' -not -path '*/zephyr/*' -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' -not -path '*/threadx-linux/*' | sort); do
        dir="$(dirname "$toml")"
        platform="$(echo "$dir" | cut -d/ -f2)"
        flags=""
        env_prefix=""
        if [ "$platform" != "native" ]; then flags="--release"; fi
        # ESP32 WiFi examples need SSID/PASSWORD env vars and nightly toolchain
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
        fi
        echo "  check $dir"
        (cd "$dir" && cargo +nightly fmt --check && eval $env_prefix cargo clippy $flags -- {{CLIPPY_LINTS}})
    done
    echo "All examples check passed!"

# Show embedded example binary sizes
size-examples-embedded: build-examples
    @echo ""
    @echo "Binary sizes (release):"
    @echo "======================="
    @size packages/reference/stm32f4-porting/rtic/target/thumbv7em-none-eabihf/release/stm32f4-rs-rtic-example 2>/dev/null || echo "RTIC: build failed"
    @size examples/stm32f4/rust/core/embassy/target/thumbv7em-none-eabihf/release/stm32f4-rs-embassy-example 2>/dev/null || echo "Embassy: build failed"
    @size packages/reference/stm32f4-porting/polling/target/thumbv7em-none-eabihf/release/stm32f4-rs-polling-example 2>/dev/null || echo "Polling: build failed"
    @size examples/stm32f4/rust/standalone/smoltcp/target/thumbv7em-none-eabihf/release/stm32f4-smoltcp 2>/dev/null || echo "stm32f4-smoltcp: build failed"

# Clean all example build artifacts
clean-examples: clean-examples-c
    #!/usr/bin/env bash
    for toml in $(find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' -not -path '*/generated/*'); do
        rm -rf "$(dirname "$toml")/target"
    done
    echo "All example build artifacts cleaned"

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
    echo "  Building zephyr/rust/zenoh/talker -> build-talker/"
    west build -b native_sim/native/64 -d build-talker -p auto nros/examples/zephyr/rust/zenoh/talker
    echo "  Building zephyr/rust/zenoh/listener -> build-listener/"
    west build -b native_sim/native/64 -d build-listener -p auto nros/examples/zephyr/rust/zenoh/listener
    echo "  Building zephyr/rust/zenoh/service-server -> build-service-server/"
    west build -b native_sim/native/64 -d build-service-server -p auto nros/examples/zephyr/rust/zenoh/service-server
    echo "  Building zephyr/rust/zenoh/service-client -> build-service-client/"
    west build -b native_sim/native/64 -d build-service-client -p auto nros/examples/zephyr/rust/zenoh/service-client
    echo "  Building zephyr/rust/zenoh/action-server -> build-action-server/"
    west build -b native_sim/native/64 -d build-action-server -p auto nros/examples/zephyr/rust/zenoh/action-server
    echo "  Building zephyr/rust/zenoh/action-client -> build-action-client/"
    west build -b native_sim/native/64 -d build-action-client -p auto nros/examples/zephyr/rust/zenoh/action-client
    echo "  Building zephyr/rust/zenoh/async-service -> build-async-service/"
    west build -b native_sim/native/64 -d build-async-service -p auto nros/examples/zephyr/rust/zenoh/async-service
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
    echo "  Building zephyr/c/zenoh/talker -> build-c-talker/"
    west build -b native_sim/native/64 -d build-c-talker -p auto nros/examples/zephyr/c/zenoh/talker
    echo "  Building zephyr/c/zenoh/listener -> build-c-listener/"
    west build -b native_sim/native/64 -d build-c-listener -p auto nros/examples/zephyr/c/zenoh/listener
    echo "Zephyr C examples built successfully!"

# Build Zephyr XRCE examples (Rust + C for XRCE-DDS backend)
build-zephyr-xrce:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr XRCE examples in $WORKSPACE..."
    cd "$WORKSPACE"
    echo "  Building zephyr/rust/xrce/talker -> build-xrce-rs-talker/"
    west build -b native_sim/native/64 -d build-xrce-rs-talker -p auto nros/examples/zephyr/rust/xrce/talker
    echo "  Building zephyr/rust/xrce/listener -> build-xrce-rs-listener/"
    west build -b native_sim/native/64 -d build-xrce-rs-listener -p auto nros/examples/zephyr/rust/xrce/listener
    echo "  Building zephyr/c/xrce/talker -> build-xrce-c-talker/"
    west build -b native_sim/native/64 -d build-xrce-c-talker -p auto nros/examples/zephyr/c/xrce/talker
    echo "  Building zephyr/c/xrce/listener -> build-xrce-c-listener/"
    west build -b native_sim/native/64 -d build-xrce-c-listener -p auto nros/examples/zephyr/c/xrce/listener
    echo "Zephyr XRCE examples built successfully!"

# Build all Zephyr examples (Rust + C, zenoh + XRCE)
build-zephyr-all: build-zephyr build-zephyr-c build-zephyr-xrce
    @echo "All Zephyr examples built!"

# Clean Zephyr build directories
clean-zephyr:
    #!/usr/bin/env bash
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    rm -rf "$WORKSPACE/build-talker" "$WORKSPACE/build-listener" "$WORKSPACE/build-service-server" "$WORKSPACE/build-service-client" "$WORKSPACE/build-action-server" "$WORKSPACE/build-action-client" "$WORKSPACE/build-async-service" "$WORKSPACE/build-c-talker" "$WORKSPACE/build-c-listener" "$WORKSPACE/build-xrce-rs-talker" "$WORKSPACE/build-xrce-rs-listener" "$WORKSPACE/build-xrce-c-talker" "$WORKSPACE/build-xrce-c-listener"
    echo "Zephyr build directories cleaned"

# Force rebuild Zephyr examples
rebuild-zephyr: clean-zephyr build-zephyr

# =============================================================================
# Examples - QEMU (Cortex-M3)
# =============================================================================

# Build QEMU ARM examples (auto-discovered from examples/qemu-arm/)
build-examples-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Building QEMU ARM examples..."
    for toml in $(find examples/qemu-arm -mindepth 3 -name Cargo.toml -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        dir="$(dirname "$toml")"
        echo "  build $dir"
        (cd "$dir" && cargo build --release)
    done
    # Also build qemu-smoltcp-bridge (library in packages/reference/)
    (cd packages/reference/qemu-smoltcp-bridge && cargo build --release)

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
    for ex in talker listener; do
        echo "  Building esp32/rust/zenoh/$ex..."
        (cd examples/esp32/rust/zenoh/$ex && SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" cargo +nightly build --release)
    done
    echo "ESP32 examples built!"

# Build ESP32 QEMU examples (requires nightly; zenoh-pico is built inline)
build-examples-esp32-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Building ESP32 QEMU examples..."
    for ex in talker listener; do
        echo "  Building qemu-esp32/rust/zenoh/$ex..."
        (cd examples/qemu-esp32/rust/zenoh/$ex && cargo +nightly build --release)
    done
    echo ""
    echo "Creating flash images..."
    mkdir -p build/esp32-qemu
    for ex in talker listener; do
        bin_name="esp32-qemu-$ex"
        elf="examples/qemu-esp32/rust/zenoh/$ex/target/riscv32imc-unknown-none-elf/release/$bin_name"
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
    if grep -q "nros ESP32-C3 QEMU BSP" "$tmpfile"; then
        echo "[PASS] ESP32-C3 QEMU boot test - BSP initialized"
    else
        echo "[FAIL] ESP32-C3 QEMU boot test - BSP banner not found"
        exit 1
    fi

# Run ESP32-C3 QEMU integration tests (build, boot, E2E via nextest)
test-qemu-esp32 verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test esp32_emulator --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Build NuttX QEMU ARM virt examples (requires nuttx in external/)
build-examples-nuttx:
    #!/usr/bin/env bash
    set -e
    echo "Building NuttX QEMU ARM virt examples..."
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just setup-nuttx"
        exit 1
    fi
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-arm-nuttx/rust/zenoh/$example && cargo +nightly build --release)
    done
    echo "NuttX QEMU examples built!"

# Run NuttX QEMU integration tests (build + verification via nextest)
test-nuttx verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just setup-nuttx"
        exit 1
    fi
    args=(-p nros-tests --test nuttx_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Build FreeRTOS QEMU MPS2-AN385 examples (requires freertos-kernel + lwip in external/)
build-examples-freertos:
    #!/usr/bin/env bash
    set -e
    echo "Building FreeRTOS QEMU MPS2-AN385 examples..."
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just setup-freertos"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just setup-freertos"
        exit 1
    fi
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-arm-freertos/rust/zenoh/$example && cargo build --release)
    done
    echo "FreeRTOS QEMU examples built!"

# Run FreeRTOS QEMU integration tests (build + verification via nextest)
test-freertos verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just setup-freertos"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just setup-freertos"
        exit 1
    fi
    args=(-p nros-tests --test freertos_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run ThreadX Linux integration tests (build + verification via nextest)
test-threadx-linux verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ]; then
        echo "ERROR: ThreadX not found at $THREADX_DIR. Run: just setup-threadx"
        exit 1
    fi
    if [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "ERROR: NetX Duo not found at $NETX_DIR. Run: just setup-threadx"
        exit 1
    fi
    args=(-p nros-tests --test threadx_linux --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run basic QEMU test (nros serialization on Cortex-M3)
test-qemu-basic verbose="": build-examples-qemu _init-test-logs
    ./tests/run-test.sh --name qemu-basic --log {{LOG_DIR}}/latest/qemu-basic.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu-arm/rust/core/cdr-test/target/thumbv7m-none-eabi/release/qemu-rs-test

# Run WCET benchmark on QEMU (DWT cycle counter)
test-qemu-wcet verbose="": build-examples-qemu _init-test-logs
    ./tests/run-test.sh --name qemu-wcet-bench --log {{LOG_DIR}}/latest/qemu-wcet-bench.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu-arm/rust/core/wcet-bench/target/thumbv7m-none-eabi/release/qemu-rs-wcet-bench

# Run LAN9118 Ethernet driver test (mps2-an385)
test-qemu-lan9118 verbose="": build-examples-qemu _init-test-logs
    ./tests/run-test.sh --name qemu-lan9118 --log {{LOG_DIR}}/latest/qemu-lan9118.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu-arm/rust/standalone/lan9118/target/thumbv7m-none-eabi/release/qemu-rs-lan9118

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
    echo "  Terminal 2: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary examples/qemu-arm/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker"
    echo "  Terminal 3: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 --binary examples/qemu-arm/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener"
    echo ""
    echo "Binaries built at:"
    echo "  examples/qemu-arm/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker"
    echo "  examples/qemu-arm/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener"
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
#   just show-asm nros-serdes 'CdrWriter::write_string'
#   just show-asm nros-serdes 'CdrWriter::write_string' thumbv7m-none-eabi
#   just show-asm nros-core 'Duration::from_nanos'
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
check-stack example="examples/qemu-arm/rust/core/wcet-bench" top="30":
    ./scripts/stack-analysis.sh {{example}} --top {{top}}

# Analyze stack usage of a pre-built ELF (e.g. Zephyr west build output)
# Usage: just check-stack-elf <path-to-elf> [top]
check-stack-elf elf top="30":
    ./scripts/stack-analysis.sh --elf {{elf}} --top {{top}}

# Analyze stack usage of C examples (requires cmake + gcc)
# Usage: just check-stack-c [example-dir] [top]
# Default: examples/native/c-talker, top 30
check-stack-c example="examples/native/c/zenoh/talker" top="30":
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
        examples/qemu-arm/rust/core/wcet-bench \
        examples/qemu-arm/rust/core/cdr-test \
        examples/qemu-arm/rust/zenoh/talker \
        examples/qemu-arm/rust/zenoh/listener \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # Rust examples (native — exclude tracing/regex infrastructure noise)
    for example in \
        examples/native/rust/zenoh/talker \
        examples/native/rust/zenoh/listener \
        examples/native/rust/zenoh/custom-msg \
        examples/native/rust/zenoh/service-server \
        examples/native/rust/zenoh/service-client \
        examples/native/rust/zenoh/action-server \
        examples/native/rust/zenoh/action-client \
    ; do
        echo "================================================================"
        ./scripts/stack-analysis.sh "$example" --top {{top}} --exclude "regex_automata|regex_syntax|aho_corasick|env_filter|env_logger|driftsort" || { echo "[FAIL] $example"; failed=$((failed + 1)); }
        echo ""
    done
    # C examples (native)
    for example in \
        examples/native/c/zenoh/talker \
        examples/native/c/zenoh/listener \
        examples/native/c/zenoh/custom-msg \
        examples/native/c/zenoh/baremetal-demo \
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
    for crate in nros-serdes nros-core nros-params nros-ghost-types nros-node; do
        echo ""
        echo "--- Verifying $crate ---"
        cargo kani -p "$crate" || { echo "[FAIL] $crate"; failed=$((failed + 1)); }
    done
    echo ""
    echo "--- Verifying nros-c ---"
    cargo kani -p nros-c --features "rmw-zenoh,platform-posix,ros-humble" || { echo "[FAIL] nros-c"; failed=$((failed + 1)); }
    echo ""
    if [ "$failed" -gt 0 ]; then
        echo "[FAIL] $failed crate(s) failed verification"
        exit 1
    fi
    echo "[OK] All Kani proofs verified"

# Run Verus unbounded deductive verification (requires Verus toolchain)
# Proves properties for ALL inputs using Z3 SMT solver
verify-verus:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Verus Verification ==="
    VERUS_DIR="$(pwd)/tools"
    if [ ! -x "$VERUS_DIR/verus" ]; then
        echo "Verus not found at $VERUS_DIR/verus"
        echo "Run 'just setup-verus' to install"
        exit 1
    fi
    export PATH="$VERUS_DIR:$PATH"
    cd packages/verification/nros-verification
    cargo verus verify
    echo "[OK] All Verus proofs verified"

# Run all verification: Kani bounded model checking + Verus deductive verification
verify: verify-kani verify-verus

# Run branch coverage on safety-critical crates (requires nightly + cargo-llvm-cov)
# MC/DC is attempted first; falls back to branch-only if unsupported
coverage:
    #!/usr/bin/env bash
    set -euo pipefail

    if ! command -v cargo-llvm-cov &>/dev/null; then
        echo "ERROR: cargo-llvm-cov not found. Install with: cargo install cargo-llvm-cov --locked"
        exit 1
    fi

    CRATES=("nros-rmw --features safety-e2e" "nros-serdes" "nros-core")
    OUTPUT_DIR="target/llvm-cov/html"

    echo "=== Branch Coverage (safety-critical crates) ==="
    echo ""

    # Clean once at start so --no-clean preserves each crate's HTML output
    cargo +nightly llvm-cov clean --workspace

    for entry in "${CRATES[@]}"; do
        crate=$(echo "$entry" | awk '{print $1}')
        extra_args=$(echo "$entry" | cut -d' ' -sf2-)
        report_dir="$OUTPUT_DIR/$crate"
        mkdir -p "$report_dir"

        echo "--- $crate ---"

        # Try MC/DC first (--mcdc implies branch), fall back to branch-only
        # --no-clean preserves HTML from prior crate runs
        if cargo +nightly llvm-cov test --no-clean \
            -p "$crate" $extra_args \
            --mcdc \
            --html --output-dir "$report_dir" 2>/dev/null; then
            echo "  [OK] MC/DC + branch coverage → $report_dir/"
        else
            echo "  [INFO] MC/DC not supported on this toolchain, using branch coverage"
            cargo +nightly llvm-cov test --no-clean \
                -p "$crate" $extra_args \
                --branch \
                --html --output-dir "$report_dir"
            echo "  [OK] Branch coverage → $report_dir/"
        fi
        echo ""
    done

    echo "=== Coverage reports: $OUTPUT_DIR/ ==="

# =============================================================================
# Zenoh
# =============================================================================

# Build zenoh transport
build-zenoh:
    cargo build -p nros-rmw --features std

# Check zenoh transport
check-zenoh:
    cargo clippy -p nros-rmw --features std -- {{CLIPPY_LINTS}}

# Build zenohd 1.6.2 from submodule (version-matched to rmw_zenoh_cpp)
build-zenohd:
    ./scripts/zenohd/build.sh

# Clean zenohd build
clean-zenohd:
    ./scripts/zenohd/build.sh --clean

# Build Micro-XRCE-DDS Agent from source (for XRCE integration tests)
build-xrce-agent:
    ./scripts/xrce-agent/build.sh

# Clean XRCE Agent build
clean-xrce-agent:
    ./scripts/xrce-agent/build.sh --clean

# Build zenoh-pico C library (standalone, for debugging)
build-zenoh-pico:
    @echo "Building zenoh-pico..."
    cd packages/zpico/zpico-sys/zenoh-pico && mkdir -p build && cd build && cmake .. -DBUILD_SHARED_LIBS=OFF && make
    @echo "zenoh-pico built at: packages/zpico/zpico-sys/zenoh-pico/build"

# =============================================================================
# Benchmarks
# =============================================================================

# Run executor fairness benchmark (requires zenohd on tcp/127.0.0.1:7447)
bench-fairness:
    cd examples/native/rust/zenoh/fairness-bench && \
        RUST_LOG=warn cargo run --release

# =============================================================================
# Integration Tests (requires zenohd running on tcp/127.0.0.1:7447)
# =============================================================================

# Run integration tests only (requires zenohd)
# Note: `just test` covers these plus workspace unit tests in a single nextest run.
# Use this when you only want to re-run integration tests.
# Excludes zephyr, rmw_interop, large_msg tests (run via test-zephyr / test-ros2 / test-large-msg)
test-integration verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast
          -E 'not binary(zephyr) and not binary(rmw_interop) and not binary(xrce_ros2_interop) and not binary(esp32_emulator) and not binary(large_msg)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run large message & throughput stress tests (requires zenohd + XRCE Agent + qemu-system-arm)
test-large-msg verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test large_msg --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run RMW abstraction integration tests (requires zenohd)
test-rmw verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test rmw --features rmw --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# =============================================================================
# Zephyr Tests (requires west workspace + bridge network)
# =============================================================================

# Run Zephyr E2E tests (requires pre-built Zephyr examples + bridge network)
# Note: thread limit handled by [test-groups.zephyr] in .config/nextest.toml
test-zephyr verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test zephyr --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

# Run Zephyr tests with full rebuild
test-zephyr-full verbose="": build-zephyr
    just test-zephyr {{verbose}}

# Run Zephyr XRCE E2E tests (requires pre-built XRCE examples + bridge network + XRCE Agent)
test-zephyr-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test zephyr --no-fail-fast -E 'test(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

# Run Zephyr C examples test
test-zephyr-c: build-zenohd
    ./tests/zephyr/run-c.sh

# =============================================================================
# ROS 2 Interop Tests (requires ROS 2 + rmw_zenoh_cpp + zenohd)
# =============================================================================

# Run ROS 2 interop tests (Rust test harness)
test-ros2 verbose="": build-zenohd
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test rmw_interop --no-fail-fast)
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

# Run XRCE-DDS integration tests (requires: just build-xrce-agent)
test-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# XRCE ↔ ROS 2 DDS interop tests (needs XRCE Agent + ROS 2 + rmw_fastrtps)
test-xrce-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(xrce_ros2_interop)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run C codegen tests only (shell-based, no nextest)
_test-c-codegen verbose="": _init-test-logs
    #!/usr/bin/env bash
    set -e
    v="{{ if verbose != "" { "--verbose" } else { "" } }}"
    ./tests/run-test.sh --name c-codegen --log {{LOG_DIR}}/latest/c-codegen.log $v -- \
        bash -c 'cd packages/codegen/packages && cargo test -p cargo-nano-ros --test test_generate_c -- --nocapture'
    ./tests/run-test.sh --name c-msg-gen --log {{LOG_DIR}}/latest/c-msg-gen.log $v -- ./tests/c-msg-gen-tests.sh

# Run all C tests (integration + codegen)
test-c verbose="": build-zenohd _init-test-logs
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(c_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    just _test-c-codegen {{verbose}}

# C XRCE-DDS API integration tests (needs cmake + XRCE Agent)
test-c-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(c_xrce_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Build a single C example with CMake.
# Usage: _build-c-example <example-dir> [extra cmake args...]
[no-exit-message]
_build-c-example dir *CMAKE_ARGS:
    #!/usr/bin/env bash
    set -e
    NROS_DIR="$(pwd)/build/install/lib/cmake/NanoRos"
    echo "Building {{dir}}..."
    cd "{{dir}}" && rm -rf build && mkdir -p build && cd build
    cmake -DNanoRos_DIR="$NROS_DIR" {{CMAKE_ARGS}} ..
    make

# Build C examples only (no tests)
build-examples-c: install-local
    just _build-c-example examples/native/c/zenoh/talker
    just _build-c-example examples/native/c/zenoh/listener
    just _build-c-example examples/native/c/zenoh/custom-msg
    @echo "C examples built!"

# Build C XRCE examples only (no tests)
build-examples-c-xrce: install-local
    just _build-c-example examples/native/c/xrce/talker  "-DNANO_ROS_RMW=xrce"
    just _build-c-example examples/native/c/xrce/listener "-DNANO_ROS_RMW=xrce"
    @echo "C XRCE examples built!"

# Clean C examples build
clean-examples-c:
    rm -rf examples/native/c/zenoh/{talker,listener,custom-msg}/build
    rm -rf examples/native/c/xrce/{talker,listener}/build
    @echo "C examples build cleaned"

# =============================================================================
# Message Bindings
# =============================================================================

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

    # Internal crate (workspace member — manually maintained, do not auto-regenerate)
    # To update: run `cargo nano-ros generate-rust` in packages/interfaces/rcl-interfaces/
    # then apply nros- prefix rename to generated Cargo.toml and source files

    # Auto-discover all examples with package.xml (Rust only, not zephyr)
    for pkg in $(find examples -name package.xml -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        dir="$(dirname "$pkg")"
        echo "  $dir"
        (cd "$dir" && $NANO_ROS generate-rust)
    done

    echo "All bindings regenerated!"

# Remove generated/ directories in examples (not rcl-interfaces — it's a workspace member)
clean-bindings:
    #!/usr/bin/env bash
    set -e
    echo "Removing generated bindings..."
    # Auto-discover all generated/ directories under examples/
    for d in $(find examples -name generated -type d -not -path '*/target/*' | sort); do
        rm -rf "$d"
        echo "  removed $d"
    done
    echo "All generated bindings removed."

# Clean and regenerate all bindings from scratch
regenerate-bindings: clean-bindings generate-bindings

# =============================================================================
# Setup & Cleanup
# =============================================================================

# Pinned versions for FreeRTOS and lwIP (used by setup-freertos)
FREERTOS_KERNEL_TAG := "V11.2.0"
LWIP_TAG := "STABLE-2_2_1_RELEASE"

# Pinned NuttX version (used by setup-nuttx)
NUTTX_TAG := "nuttx-12.8.0"

# Pinned ThreadX version (used by setup-threadx)
THREADX_TAG := "v6.4.5.202504_rel"

# Download FreeRTOS kernel and lwIP sources for FreeRTOS platform development
# Sources are placed in external/ and pointed to by environment variables.
# Run this before building with --features platform-freertos.
setup-freertos:
    #!/usr/bin/env bash
    set -e
    echo "=== FreeRTOS + lwIP Setup ==="
    echo ""
    FREERTOS_DIR="$(pwd)/external/freertos-kernel"
    LWIP_DIR="$(pwd)/external/lwip"

    # --- FreeRTOS Kernel ---
    if [ -d "$FREERTOS_DIR/include" ]; then
        tag=$(cd "$FREERTOS_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "FreeRTOS kernel already present at $FREERTOS_DIR (tag: $tag)"
        if [ "$tag" != "{{FREERTOS_KERNEL_TAG}}" ]; then
            echo "  WARNING: expected {{FREERTOS_KERNEL_TAG}}, found $tag"
            echo "  To update: rm -rf $FREERTOS_DIR && just setup-freertos"
        fi
    else
        echo "Cloning FreeRTOS kernel {{FREERTOS_KERNEL_TAG}}..."
        git clone --depth 1 --branch "{{FREERTOS_KERNEL_TAG}}" \
            https://github.com/FreeRTOS/FreeRTOS-Kernel.git "$FREERTOS_DIR"
        echo "  -> $FREERTOS_DIR"
    fi
    echo ""

    # --- lwIP ---
    if [ -d "$LWIP_DIR/src/include" ]; then
        tag=$(cd "$LWIP_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "lwIP already present at $LWIP_DIR (tag: $tag)"
        if [ "$tag" != "{{LWIP_TAG}}" ]; then
            echo "  WARNING: expected {{LWIP_TAG}}, found $tag"
            echo "  To update: rm -rf $LWIP_DIR && just setup-freertos"
        fi
    else
        echo "Cloning lwIP {{LWIP_TAG}}..."
        git clone --depth 1 --branch "{{LWIP_TAG}}" \
            https://github.com/lwip-tcpip/lwip.git "$LWIP_DIR"
        echo "  -> $LWIP_DIR"
    fi
    echo ""

    echo "=== Environment Variables ==="
    echo ""
    echo "Add these to your shell or .cargo/config.toml [env] section:"
    echo ""
    echo "  export FREERTOS_DIR=$FREERTOS_DIR"
    echo "  export FREERTOS_PORT=GCC/ARM_CM3"
    echo "  export LWIP_DIR=$LWIP_DIR"
    echo "  export FREERTOS_CONFIG_DIR=<board-crate>/config"
    echo ""
    echo "For QEMU MPS2-AN385 (Cortex-M3), use FREERTOS_PORT=GCC/ARM_CM3."
    echo "For STM32F7 (Cortex-M7), use FREERTOS_PORT=GCC/ARM_CM7/r0p1."
    echo ""
    echo "Setup complete!"

# Download NuttX RTOS and apps sources for NuttX platform development.
# Sources are placed in external/ and pointed to by NUTTX_DIR environment variable.
# Run this before building with --features platform-nuttx.
setup-nuttx:
    #!/usr/bin/env bash
    set -e
    echo "=== NuttX Setup ==="
    echo ""
    NUTTX_DIR="$(pwd)/external/nuttx"
    NUTTX_APPS_DIR="$(pwd)/external/nuttx-apps"

    # --- NuttX RTOS ---
    if [ -d "$NUTTX_DIR/include" ]; then
        tag=$(cd "$NUTTX_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "NuttX already present at $NUTTX_DIR (tag: $tag)"
        if [ "$tag" != "{{NUTTX_TAG}}" ]; then
            echo "  WARNING: expected {{NUTTX_TAG}}, found $tag"
            echo "  To update: rm -rf $NUTTX_DIR && just setup-nuttx"
        fi
    else
        echo "Cloning NuttX {{NUTTX_TAG}}..."
        git clone --depth 1 --branch "{{NUTTX_TAG}}" \
            https://github.com/apache/nuttx.git "$NUTTX_DIR"
        echo "  -> $NUTTX_DIR"
    fi
    echo ""

    # --- NuttX Apps ---
    if [ -d "$NUTTX_APPS_DIR/include" ]; then
        tag=$(cd "$NUTTX_APPS_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "NuttX apps already present at $NUTTX_APPS_DIR (tag: $tag)"
        if [ "$tag" != "{{NUTTX_TAG}}" ]; then
            echo "  WARNING: expected {{NUTTX_TAG}}, found $tag"
            echo "  To update: rm -rf $NUTTX_APPS_DIR && just setup-nuttx"
        fi
    else
        echo "Cloning NuttX apps {{NUTTX_TAG}}..."
        git clone --depth 1 --branch "{{NUTTX_TAG}}" \
            https://github.com/apache/nuttx-apps.git "$NUTTX_APPS_DIR"
        echo "  -> $NUTTX_APPS_DIR"
    fi
    echo ""

    echo "=== Environment Variables ==="
    echo ""
    echo "Add these to your shell or .cargo/config.toml [env] section:"
    echo ""
    echo "  export NUTTX_DIR=$NUTTX_DIR"
    echo "  export NUTTX_APPS_DIR=$NUTTX_APPS_DIR"
    echo ""
    echo "Setup complete!"

# Download ThreadX kernel, NetX Duo, and learn-samples for ThreadX platform development.
# Sources are placed in external/ and pointed to by environment variables.
# Run this before building with --features platform-threadx.
setup-threadx:
    #!/usr/bin/env bash
    set -e
    echo "=== ThreadX + NetX Duo Setup ==="
    echo ""
    THREADX_DIR="$(pwd)/external/threadx"
    NETX_DIR="$(pwd)/external/netxduo"
    THREADX_LEARN_DIR="$(pwd)/external/threadx-learn-samples"

    # --- ThreadX Kernel ---
    if [ -d "$THREADX_DIR/common" ]; then
        tag=$(cd "$THREADX_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "ThreadX already present at $THREADX_DIR (tag: $tag)"
        if [ "$tag" != "{{THREADX_TAG}}" ]; then
            echo "  WARNING: expected {{THREADX_TAG}}, found $tag"
            echo "  To update: rm -rf $THREADX_DIR && just setup-threadx"
        fi
    else
        echo "Cloning ThreadX {{THREADX_TAG}}..."
        git clone --depth 1 --branch "{{THREADX_TAG}}" \
            https://github.com/eclipse-threadx/threadx.git "$THREADX_DIR"
        echo "  -> $THREADX_DIR"
    fi
    echo ""

    # --- NetX Duo ---
    if [ -d "$NETX_DIR/common" ]; then
        tag=$(cd "$NETX_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "NetX Duo already present at $NETX_DIR (tag: $tag)"
        if [ "$tag" != "{{THREADX_TAG}}" ]; then
            echo "  WARNING: expected {{THREADX_TAG}}, found $tag"
            echo "  To update: rm -rf $NETX_DIR && just setup-threadx"
        fi
    else
        echo "Cloning NetX Duo {{THREADX_TAG}}..."
        git clone --depth 1 --branch "{{THREADX_TAG}}" \
            https://github.com/eclipse-threadx/netxduo.git "$NETX_DIR"
        echo "  -> $NETX_DIR"
    fi
    echo ""

    # --- ThreadX Learn Samples (contains nx_linux_network_driver.c) ---
    if [ -d "$THREADX_LEARN_DIR/courses" ]; then
        echo "ThreadX learn-samples already present at $THREADX_LEARN_DIR"
    else
        echo "Cloning ThreadX learn-samples (main branch)..."
        git clone --depth 1 \
            https://github.com/eclipse-threadx/threadx-learn-samples.git "$THREADX_LEARN_DIR"
        echo "  -> $THREADX_LEARN_DIR"
    fi
    echo ""

    echo "=== Environment Variables ==="
    echo ""
    echo "Add these to your shell or .cargo/config.toml [env] section:"
    echo ""
    echo "  export THREADX_DIR=$THREADX_DIR"
    echo "  export THREADX_CONFIG_DIR=<board-crate>/config"
    echo "  export NETX_DIR=$NETX_DIR"
    echo "  export NETX_CONFIG_DIR=<board-crate>/config"
    echo ""
    echo "The Linux network driver is at:"
    echo "  $THREADX_LEARN_DIR/courses/netxduo/Driver/nx_linux_network_driver.c"
    echo ""
    echo "Setup complete!"

# Download Verus binary from GitHub releases to tools/verus
setup-verus:
    #!/usr/bin/env bash
    set -euo pipefail
    echo "=== Verus Setup ==="
    VERUS_DIR="tools"
    VERUS_BIN="$VERUS_DIR/verus"
    if [ -x "$VERUS_BIN" ]; then
        echo "Verus already installed at $VERUS_BIN"
        "$VERUS_BIN" --version
        exit 0
    fi
    # Determine platform suffix for release asset
    OS=$(uname -s)
    ARCH=$(uname -m)
    case "$OS-$ARCH" in
        Linux-x86_64)   PLATFORM="x86-linux" ;;
        Darwin-x86_64)  PLATFORM="x86-macos" ;;
        Darwin-arm64)   PLATFORM="arm64-macos" ;;
        Darwin-aarch64) PLATFORM="arm64-macos" ;;
        *)              echo "Unsupported platform: $OS-$ARCH"; exit 1 ;;
    esac
    # Query GitHub API for latest release download URL
    API_URL="https://api.github.com/repos/verus-lang/verus/releases/latest"
    echo "Querying latest Verus release..."
    DOWNLOAD_URL=$(curl -fsSL "$API_URL" | python3 -c "import sys,json;[print(a['browser_download_url']) for a in json.load(sys.stdin)['assets'] if a['name'].endswith('-${PLATFORM}.zip')]" | head -1)
    if [ -z "$DOWNLOAD_URL" ]; then
        echo "ERROR: No release asset found for platform $PLATFORM"
        exit 1
    fi
    echo "Downloading $DOWNLOAD_URL..."
    ZIPFILE="/tmp/verus-${PLATFORM}.zip"
    curl -fsSL "$DOWNLOAD_URL" -o "$ZIPFILE"
    # Extract to tools/ (zip contains verus-<platform>/ directory)
    TMPDIR=$(mktemp -d)
    unzip -q "$ZIPFILE" -d "$TMPDIR"
    mkdir -p "$VERUS_DIR"
    cp -r "$TMPDIR"/verus-${PLATFORM}/* "$VERUS_DIR/"
    rm -rf "$TMPDIR" "$ZIPFILE"
    chmod +x "$VERUS_BIN" "$VERUS_DIR/cargo-verus" "$VERUS_DIR/z3" "$VERUS_DIR/rust_verify"
    # Install required Rust toolchain
    REQUIRED_TC=$("$VERUS_BIN" --version 2>&1 | grep 'Toolchain:' | sed 's/.*Toolchain: //' || true)
    if [ -n "$REQUIRED_TC" ]; then
        echo "Installing required toolchain: $REQUIRED_TC"
        rustup toolchain install "$REQUIRED_TC"
    fi
    "$VERUS_BIN" --version
    echo "Verus setup complete."

# Install toolchains and tools (interactive — lists actions and asks for confirmation)
setup:
    #!/usr/bin/env bash
    set -e
    echo "=== nros setup ==="
    echo ""
    echo "This will:"
    echo "  1. Install system packages via apt (may prompt for sudo):"
    echo "       gcc-arm-none-eabi, qemu-system-arm, cmake, socat,"
    echo "       gcc-riscv64-unknown-elf, picolibc-riscv64-unknown-elf"
    echo "  2. Install Rust toolchains (stable + nightly)"
    echo "  3. Add rustup components: rustfmt, clippy, rust-src, miri"
    echo "  4. Add cross-compilation targets:"
    echo "       - thumbv7em-none-eabihf  (ARM Cortex-M4F)"
    echo "       - thumbv7m-none-eabi     (ARM Cortex-M3)"
    echo "       - riscv32imc-unknown-none-elf (ESP32-C3 RISC-V)"
    echo "       - armv7a-nuttx-eabi      (NuttX ARM, Tier 3 via build-std)"
    echo "  5. Install cargo tools + verification toolchains:"
    echo "       - cargo-nextest          (test runner)"
    echo "       - espflash               (ESP32 flash tool)"
    echo "       - cargo-nano-ros         (message binding generator)"
    echo "       - kani-verifier          (bounded model checking)"
    echo "       - verus                  (deductive verification)"
    echo "  6. Build Espressif QEMU from source → ~/.local/bin/qemu-system-riscv32"
    echo "     (ESP32-C3 emulator — requires git, ninja, python3, pkg-config,"
    echo "      libglib2.0-dev, libpixman-1-dev, libgcrypt20-dev, libslirp-dev)"
    echo "  7. Build Micro-XRCE-DDS Agent from source → build/xrce-agent/MicroXRCEAgent"
    echo "     (XRCE-DDS integration tests — requires cmake, g++)"
    echo "  8. Download FreeRTOS kernel + lwIP → external/freertos-kernel, external/lwip"
    echo "  9. Download NuttX RTOS + apps → external/nuttx, external/nuttx-apps"
    echo ""
    read -r -p "Proceed? [Y/n] " answer
    if [[ "$answer" =~ ^[Nn] ]]; then
        echo "Setup cancelled."
        exit 0
    fi
    echo ""

    echo "=== [1/10] System packages (apt) ==="
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
    check_apt socat                          socat
    check_apt gcc-riscv64-unknown-elf        riscv64-unknown-elf-gcc
    # mbedTLS: check via header (library package, no binary)
    if [ -f /usr/include/mbedtls/ssl.h ]; then
        printf "  %-40s %s\n" "libmbedtls-dev" "[already installed]"
    else
        apt_pkgs+=("libmbedtls-dev")
    fi
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

    echo "=== [2/10] Installing Rust toolchains ==="
    rustup toolchain install stable
    rustup toolchain install nightly
    echo ""

    echo "=== [3/10] Adding rustup components ==="
    rustup component add rustfmt clippy rust-src
    rustup component add llvm-tools
    rustup component add --toolchain nightly rustfmt miri rust-src llvm-tools
    echo ""

    echo "=== [4/10] Adding cross-compilation targets ==="
    rustup target add thumbv7em-none-eabihf
    rustup target add thumbv7m-none-eabi
    rustup target add riscv32imc-unknown-none-elf
    rustup +nightly target add thumbv7m-none-eabi
    # NuttX: armv7a-nuttx-eabi is Tier 3 — can't install via rustup, uses -Z build-std.
    # Verify the nightly compiler knows about it (rust-src installed in step 3).
    if rustc +nightly --print target-list 2>/dev/null | grep -q armv7a-nuttx-eabi; then
        echo "  armv7a-nuttx-eabi (NuttX Tier 3): supported via nightly + build-std"
    else
        echo "  WARNING: armv7a-nuttx-eabi not in nightly target list — NuttX builds may fail"
    fi
    echo ""

    echo "=== [5/10] Installing cargo tools + verification toolchains ==="
    cargo install cargo-nextest --locked
    cargo install cargo-llvm-cov --locked
    cargo install espflash --locked || echo "WARNING: espflash install failed (non-fatal)"
    cargo install rustfilt --locked || echo "WARNING: rustfilt install failed (non-fatal)"
    cargo install cargo-show-asm --locked || echo "WARNING: cargo-show-asm install failed (non-fatal)"
    if command -v cargo-kani &>/dev/null && [ -d "$HOME/.kani" ]; then
        kani_ver=$(basename "$(ls -d "$HOME"/.kani/kani-* 2>/dev/null | grep -v '\.tar' | head -1)" 2>/dev/null || true)
        echo "kani-verifier already installed ($kani_ver)"
    else
        cargo install --locked kani-verifier && cargo kani setup || echo "WARNING: kani install failed (non-fatal)"
    fi
    just setup-verus || echo "WARNING: Verus setup failed (non-fatal)"
    cargo install --path packages/codegen/packages/cargo-nano-ros --locked
    echo ""

    echo "=== [6/10] Building Espressif QEMU (qemu-system-riscv32) ==="
    if command -v qemu-system-riscv32 &>/dev/null; then
        echo "Already installed: $(qemu-system-riscv32 --version | head -1)"
        echo "Skipping build. To reinstall, run: ./scripts/esp32/install-espressif-qemu.sh"
    else
        ./scripts/esp32/install-espressif-qemu.sh
    fi
    echo ""

    echo "=== [7/10] Building Micro-XRCE-DDS Agent ==="
    if [ -f "build/xrce-agent/MicroXRCEAgent" ]; then
        echo "Already built: build/xrce-agent/MicroXRCEAgent"
        echo "To rebuild, run: just build-xrce-agent"
    else
        ./scripts/xrce-agent/build.sh || echo "WARNING: XRCE Agent build failed (non-fatal, needed for just test-xrce)"
    fi
    echo ""

    echo "=== [8/10] Downloading FreeRTOS kernel + lwIP ==="
    just setup-freertos || echo "WARNING: FreeRTOS setup failed (non-fatal, needed for just test-freertos)"
    echo ""

    echo "=== [9/10] Downloading NuttX RTOS + apps ==="
    just setup-nuttx || echo "WARNING: NuttX setup failed (non-fatal, needed for just test-nuttx)"
    echo ""

    echo "=== [10/10] Downloading ThreadX + NetX Duo ==="
    just setup-threadx || echo "WARNING: ThreadX setup failed (non-fatal, needed for just test-threadx)"
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

# Generate Rust API documentation (rustdoc)
doc-rust:
    cargo doc --workspace --no-deps

# Generate C API documentation (Doxygen)
# Requires doxygen — skips with a warning if not installed.
# The generated header must exist (run `cargo build -p nros-c` first).
doc-c:
    #!/usr/bin/env bash
    set -e
    if ! command -v doxygen &>/dev/null; then
        echo "WARNING: doxygen not found — skipping C API docs."
        echo "Install with: sudo apt install doxygen"
        exit 0
    fi
    header="packages/core/nros-c/include/nros/nros_generated.h"
    if [ ! -f "$header" ]; then
        echo "Generated header not found, building nros-c first..."
        cargo build -p nros-c --features "rmw-zenoh,platform-posix,ros-humble"
    fi
    (cd packages/core/nros-c && doxygen Doxyfile)
    echo "C API docs generated: target/doc/c-api/html/index.html"

# Verify hand-written C headers are syntactically correct.
# Signature drift against Rust is caught at link time by `just test-c`.
doc-c-check:
    #!/usr/bin/env bash
    set -e
    echo "Checking C headers for syntax errors..."
    cc -fsyntax-only \
        -Ipackages/core/nros-c/include \
        -include packages/core/nros-c/include/nros/nros.h \
        -x c /dev/null
    echo "All C headers are syntactically correct."

# Generate all documentation (Rust + C)
doc: doc-rust doc-c

# Clean all build artifacts created by `just build`
clean: clean-examples clean-zephyr clean-zenohd
    cargo clean
    rm -rf build
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
    @echo "  just build-zephyr           # Build Rust zenoh Zephyr examples"
    @echo "  just build-zephyr-c         # Build C zenoh Zephyr examples"
    @echo "  just build-zephyr-xrce      # Build XRCE Zephyr examples (Rust + C)"
    @echo "  just build-zephyr-all       # Build all Zephyr examples"
    @echo "  just rebuild-zephyr         # Clean and rebuild"
    @echo "  just clean-zephyr           # Remove all build directories"
    @echo ""
    @echo "Run tests:"
    @echo "  just test-zephyr            # Run zenoh Zephyr E2E tests"
    @echo "  just test-zephyr-xrce       # Run XRCE Zephyr E2E tests"
    @echo "  just test-zephyr-full       # Rebuild and run zenoh Zephyr tests"
    @echo "  just test-zephyr-c          # Run Zephyr C examples test"
    @echo ""
    @echo "Manual build (from Zephyr workspace):"
    @echo "  west build -b native_sim/native/64 -d build-talker nros/examples/zephyr/rust/zenoh/talker"
    @echo "  west build -b native_sim/native/64 -d build-listener nros/examples/zephyr/rust/zenoh/listener"
    @echo "  west build -b native_sim/native/64 -d build-xrce-rs-talker nros/examples/zephyr/rust/xrce/talker"
    @echo "  west build -b native_sim/native/64 -d build-xrce-rs-listener nros/examples/zephyr/rust/xrce/listener"

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
        -v nros-cargo-registry:/cargo/registry \
        -v nros-cargo-git:/cargo/git \
        nano-ros-qemu

# Start Docker container with TAP networking support
docker-shell-network:
    @echo "Starting Docker container with networking support..."
    docker run -it --rm \
        --cap-add=NET_ADMIN \
        --device=/dev/net/tun \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nros-cargo-registry:/cargo/registry \
        -v nros-cargo-git:/cargo/git \
        nano-ros-qemu

# Run bare-metal QEMU talker/listener test using Docker Compose
# Uses separate containers for zenohd, talker, and listener with isolated networking
# Each container creates its own TAP/bridge and NATs to the Docker network
test-docker-qemu: docker-build build-examples-qemu
    @echo "Running bare-metal QEMU talker/listener test..."
    @echo "This starts 3 containers: zenohd, talker, and listener"
    @echo ""
    HOST_UID=$(id -u) HOST_GID=$(id -g) docker compose -f tests/qemu-baremetal/docker-compose.yml up --build --abort-on-container-exit
    @docker compose -f tests/qemu-baremetal/docker-compose.yml down -v 2>/dev/null || true

# Run QEMU build inside Docker container
docker-build-qemu: docker-build
    @echo "Building QEMU examples inside Docker..."
    docker run --rm \
        -e HOST_UID=$(id -u) -e HOST_GID=$(id -g) \
        -v $(pwd):/work \
        -v nros-cargo-registry:/cargo/registry \
        -v nros-cargo-git:/cargo/git \
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
    @echo "  just test-docker-qemu       # Run QEMU talker/listener test"
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
