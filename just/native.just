set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# CLIPPY_LINTS removed — workspace-wide lint config now lives in root Cargo.toml `[workspace.lints]`

LOG_DIR := "test-logs"
NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# =============================================================================
# Rust examples (auto-discovered from examples/{platform}/{lang}/{rmw}/{use-case}/)
# =============================================================================

# Build all Rust examples (auto-discovered, excludes zephyr + cross-compiled RTOS).
#
# Parallelised via GNU parallel — examples are independent standalone
# crates (per the copy-out invariant in CLAUDE.md), so each cargo
# invocation can run concurrently. JOBS defaults to 75 % of CPU cores
# to leave headroom for rustc's per-process RAM (~1-3 GB). Override
# with `just native build JOBS=4` for tighter machines.
build JOBS="75%":
    #!/usr/bin/env bash
    set -e
    echo "Building examples (parallel, JOBS={{JOBS}})..."
    if ! command -v parallel >/dev/null 2>&1; then
        echo "GNU parallel not found — falling back to serial build."
        SERIAL=1
    else
        SERIAL=0
    fi

    find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' \
                       -not -path '*/build/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' \
                       -not -path '*/px4/*' | sort > /tmp/_nros_example_toml.list

    build_one() {
        local toml="$1"
        local dir="$(dirname "$toml")"
        local platform="$(echo "$dir" | cut -d/ -f2)"
        local flags=""
        local env_prefix=""
        local toolchain=""
        if [ "$platform" != "native" ]; then flags="--release"; fi
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32-baremetal" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
            toolchain="+{{NIGHTLY}}"
        fi
        echo "  build $dir"
        ( cd "$dir" && eval $env_prefix cargo $toolchain build $flags )
    }
    export -f build_one
    export NIGHTLY="{{NIGHTLY}}"

    if [ "$SERIAL" = "1" ]; then
        while read -r toml; do build_one "$toml"; done < /tmp/_nros_example_toml.list
    else
        parallel --halt now,fail=1 --line-buffer -j {{JOBS}} build_one :::: /tmp/_nros_example_toml.list
    fi
    rm -f /tmp/_nros_example_toml.list
    echo "All examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
build-fixtures:
    #!/usr/bin/env bash
    set -e
    echo "Building native test fixtures..."
    # Talker is built with `--features param-services` so the params tests
    # see "Counter start value: 0" when the talker reads its declared param.
    # Same binary path as the default-feature talker, just feature-augmented.
    echo "  → native/rust/zenoh/talker (--features param-services)"
    ( cd examples/native/rust/zenoh/talker && \
      cargo build --release --features param-services --quiet )
    examples=(
        native/rust/zenoh/listener
        native/rust/zenoh/lifecycle-node
        native/rust/zenoh/custom-msg
        native/rust/zenoh/service-server
        native/rust/zenoh/service-client
        native/rust/zenoh/action-server
        native/rust/zenoh/action-client
        native/rust/zenoh/rtic-talker
        native/rust/zenoh/rtic-listener
        native/rust/zenoh/rtic-service-server
        native/rust/zenoh/rtic-service-client
        native/rust/zenoh/rtic-action-server
        native/rust/zenoh/rtic-action-client
        native/rust/zenoh/stress-test
        native/rust/zenoh/fairness-bench
        native/rust/xrce/talker
        native/rust/xrce/listener
        native/rust/xrce/service-server
        native/rust/xrce/service-client
        native/rust/xrce/action-server
        native/rust/xrce/action-client
        native/rust/xrce/large-msg-test
        native/rust/xrce/serial-talker
        native/rust/xrce/serial-listener
        native/rust/xrce/stress-test
        native/rust/dds/talker
        native/rust/dds/listener
        native/rust/dds/service-server
        native/rust/dds/service-client
        native/rust/dds/action-server
        native/rust/dds/action-client
    )
    for ex in "${examples[@]}"; do
        echo "  → $ex"
        ( cd "examples/$ex" && cargo build --release --quiet ) &
    done
    wait
    echo "  → native/rust/zenoh/{talker,listener} (--features link-tls)"
    ( cd examples/native/rust/zenoh/talker && \
      cargo build --release --features link-tls --target-dir target-tls --quiet ) &
    ( cd examples/native/rust/zenoh/listener && \
      cargo build --release --features link-tls --target-dir target-tls --quiet ) &
    wait
    echo "  → native/rust/zenoh/{talker,listener} (--features safety-e2e)"
    ( cd examples/native/rust/zenoh/talker && \
      cargo build --release --features safety-e2e --target-dir target-safety --quiet ) &
    ( cd examples/native/rust/zenoh/listener && \
      cargo build --release --features safety-e2e --target-dir target-safety --quiet ) &
    wait
    echo "  → native/rust/zenoh/listener (--features unstable-zenoh-api)"
    ( cd examples/native/rust/zenoh/listener && \
      cargo build --release --features unstable-zenoh-api --target-dir target-zero-copy --quiet )
    echo "  → native/rust/zenoh/stress-test (ZPICO_SUBSCRIBER_BUFFER_SIZE=8192)"
    ( cd examples/native/rust/zenoh/stress-test && \
      ZPICO_SUBSCRIBER_BUFFER_SIZE=8192 cargo build --release --target-dir target-large-buf --quiet )

# Format all Rust examples (auto-discovered, parallel)
format:
    #!/usr/bin/env bash
    set -e
    echo "Formatting examples..."
    find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' \
        -not -path '*/generated/*' -not -path '*/zephyr/*' \
        -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
        -not -path '*/threadx-linux/*' \
        -not -path '*/qemu-riscv64-threadx/*' \
        -exec dirname {} \; | sort \
    | parallel --halt now,fail=1 --line-buffer \
        'cd {} && cargo +{{NIGHTLY}} fmt && echo "  fmt {}"'
    echo "All examples formatted!"

# Check all Rust examples (auto-discovered, parallel via GNU parallel).
check JOBS="75%":
    #!/usr/bin/env bash
    set -e
    echo "Checking examples (parallel, JOBS={{JOBS}})..."
    if ! command -v parallel >/dev/null 2>&1; then
        echo "GNU parallel not found — falling back to serial check."
        SERIAL=1
    else
        SERIAL=0
    fi

    find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' \
                       -not -path '*/build/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' \
                       -not -path '*/px4/*' | sort > /tmp/_nros_check_toml.list

    check_one() {
        local toml="$1"
        local dir="$(dirname "$toml")"
        local platform="$(echo "$dir" | cut -d/ -f2)"
        local flags=""
        local env_prefix=""
        if [ "$platform" != "native" ]; then flags="--release"; fi
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32-baremetal" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
        fi
        echo "  check $dir"
        ( cd "$dir" && cargo +{{NIGHTLY}} fmt --check && eval $env_prefix cargo clippy $flags )
    }
    export -f check_one
    export NIGHTLY="{{NIGHTLY}}"

    if [ "$SERIAL" = "1" ]; then
        while read -r toml; do check_one "$toml"; done < /tmp/_nros_check_toml.list
    else
        parallel --halt now,fail=1 --line-buffer -j {{JOBS}} check_one :::: /tmp/_nros_check_toml.list
    fi
    rm -f /tmp/_nros_check_toml.list
    echo "All examples check passed!"

# Show embedded example binary sizes
size: build
    @echo ""
    @echo "Binary sizes (release):"
    @echo "======================="
    @size packages/reference/stm32f4-porting/rtic/target/thumbv7em-none-eabihf/release/stm32f4-rs-rtic-example 2>/dev/null || echo "RTIC: build failed"
    @size examples/stm32f4/rust/core/embassy/target/thumbv7em-none-eabihf/release/stm32f4-rs-embassy-example 2>/dev/null || echo "Embassy: build failed"
    @size packages/reference/stm32f4-porting/polling/target/thumbv7em-none-eabihf/release/stm32f4-rs-polling-example 2>/dev/null || echo "Polling: build failed"
    @size examples/stm32f4/rust/standalone/smoltcp/target/thumbv7em-none-eabihf/release/stm32f4-smoltcp 2>/dev/null || echo "stm32f4-smoltcp: build failed"

# Clean all example build artifacts
clean: clean-c clean-cpp
    #!/usr/bin/env bash
    for toml in $(find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*'); do
        rm -rf "$(dirname "$toml")/target"
    done
    echo "All example build artifacts cleaned"

# =============================================================================
# Native C/C++ examples (CMake-based)
# =============================================================================

# Build a single C example with CMake
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
build-c:
    #!/usr/bin/env bash
    set -e
    just install-local-posix
    just native _build-c-example examples/native/c/zenoh/talker
    just native _build-c-example examples/native/c/zenoh/listener
    just native _build-c-example examples/native/c/zenoh/custom-msg
    echo "C examples built!"

# Build C XRCE examples only (no tests)
build-c-xrce:
    #!/usr/bin/env bash
    set -e
    just install-local-posix
    just native _build-c-example examples/native/c/xrce/talker  "-DNANO_ROS_RMW=xrce"
    just native _build-c-example examples/native/c/xrce/listener "-DNANO_ROS_RMW=xrce"
    echo "C XRCE examples built!"

# Clean C examples build
clean-c:
    rm -rf examples/native/c/zenoh/{talker,listener,custom-msg}/build
    rm -rf examples/native/c/xrce/{talker,listener}/build
    @echo "C examples build cleaned"

# Build a single C++ example with CMake
[no-exit-message]
_build-cpp-example dir *CMAKE_ARGS:
    #!/usr/bin/env bash
    set -e
    NROS_DIR="$(pwd)/build/install/lib/cmake/NanoRos"
    echo "Building {{dir}}..."
    cd "{{dir}}" && rm -rf build && mkdir -p build && cd build
    cmake -DNanoRos_DIR="$NROS_DIR" {{CMAKE_ARGS}} ..
    make

# Build C++ examples only (no tests)
build-cpp:
    #!/usr/bin/env bash
    set -e
    just install-local-posix
    just native _build-cpp-example examples/native/cpp/zenoh/talker
    just native _build-cpp-example examples/native/cpp/zenoh/listener
    just native _build-cpp-example examples/native/cpp/zenoh/service-server
    just native _build-cpp-example examples/native/cpp/zenoh/service-client
    just native _build-cpp-example examples/native/cpp/zenoh/action-server
    just native _build-cpp-example examples/native/cpp/zenoh/action-client
    echo "C++ examples built!"

# Clean C++ examples build
clean-cpp:
    rm -rf examples/native/cpp/zenoh/{talker,listener,service-server,service-client,action-server,action-client}/build
    @echo "C++ examples build cleaned"

# =============================================================================
# Integration tests
# =============================================================================

# Run integration tests only (requires zenohd)
test verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --no-fail-fast \
          -E 'not binary(zephyr) and not binary(rmw_interop) and not binary(xrce_ros2_interop) and not binary(esp32_emulator) and not binary(large_msg)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run all native tests (integration + ROS 2 interop + large_msg + C/C++ API)
test-all verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just native test {{verbose}} || failed=1
    just native test-large-msg {{verbose}} || failed=1
    just native test-ros2 {{verbose}} || failed=1
    just native test-c {{verbose}} || failed=1
    just native test-cpp {{verbose}} || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some native tests failed."
        exit 1
    fi

# CI: check + test
ci: check test

# Run large message & throughput stress tests
test-large-msg verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --test large_msg --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run RMW abstraction integration tests
test-rmw verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --test rmw --features rmw --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run ROS 2 interop tests (Rust test harness)
test-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --test rmw_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run the `ros2 lifecycle …` interop test against an nros lifecycle-node.
# Requires `just rmw_zenoh setup` first so rmw_zenoh_cpp matches our zenoh-pico pin.
test-ros2-lifecycle verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --test ros2_lifecycle_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run ROS 2 interop tests (shell script)
test-ros2-shell:
    ./tests/ros2-interop.sh

# Run DDS ROS 2 interop tests (requires ROS 2 + rmw_cyclonedds_cpp)
test-dds-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test dds_ros2_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run C codegen tests only (shell-based, no nextest)
_test-c-codegen verbose="":
    #!/usr/bin/env bash
    set -e
    just init-test-logs
    v="{{ if verbose != "" { "--verbose" } else { "" } }}"
    ./tests/run-test.sh --name c-codegen --log {{LOG_DIR}}/latest/c-codegen.log $v -- \
        bash -c 'cd packages/codegen/packages && cargo test -p cargo-nano-ros --test test_generate_c -- --nocapture'
    ./tests/run-test.sh --name c-msg-gen --log {{LOG_DIR}}/latest/c-msg-gen.log $v -- ./tests/c-msg-gen-tests.sh

# Run all C tests (integration + codegen)
test-c verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    just init-test-logs
    args=(-p nros-tests --no-fail-fast -E 'binary(c_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    just native _test-c-codegen {{verbose}}

# C++ API integration tests (needs cmake + zenohd + install-local)
test-cpp verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --no-fail-fast -E 'binary(cpp_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# C XRCE-DDS API integration tests (needs cmake + XRCE Agent)
test-c-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(c_xrce_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run executor fairness benchmark (requires zenohd)
bench-fairness:
    cd examples/native/rust/zenoh/fairness-bench && \
        RUST_LOG=warn cargo run --release

# =============================================================================
# Run examples
# =============================================================================

# Run native Rust talker
talker:
    cd examples/native/rust/zenoh/talker && cargo run

# Run native Rust listener
listener:
    cd examples/native/rust/zenoh/listener && cargo run

# Run native Rust service server
service-server:
    cd examples/native/rust/zenoh/service-server && cargo run

# Run native Rust service client
service-client:
    cd examples/native/rust/zenoh/service-client && cargo run

# Run native Rust action server
action-server:
    cd examples/native/rust/zenoh/action-server && cargo run

# Run native Rust action client
action-client:
    cd examples/native/rust/zenoh/action-client && cargo run

# Start zenohd router for native examples (port 7447)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7447 --no-multicast-scouting
