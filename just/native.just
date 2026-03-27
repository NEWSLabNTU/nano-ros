set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
CLIPPY_LINTS := "-D warnings -D clippy::infinite_iter -D clippy::while_immutable_condition -D clippy::never_loop -D clippy::empty_loop -D clippy::unconditional_recursion -W clippy::large_stack_arrays -W clippy::large_types_passed_by_value"
LOG_DIR := "test-logs"

# =============================================================================
# Rust examples (auto-discovered from examples/{platform}/{lang}/{rmw}/{use-case}/)
# =============================================================================

# Build all Rust examples (auto-discovered, excludes zephyr + cross-compiled RTOS)
build:
    #!/usr/bin/env bash
    set -e
    echo "Building examples..."
    for toml in $(find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' \
                       -not -path '*/build/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' | sort); do
        dir="$(dirname "$toml")"
        platform="$(echo "$dir" | cut -d/ -f2)"
        flags=""
        env_prefix=""
        toolchain=""
        if [ "$platform" != "native" ]; then flags="--release"; fi
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32-baremetal" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
            toolchain="+nightly"
        fi
        echo "  build $dir"
        (cd "$dir" && eval $env_prefix cargo $toolchain build $flags)
    done
    echo "All examples built!"

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
        'cd {} && cargo +nightly fmt && echo "  fmt {}"'
    echo "All examples formatted!"

# Check all Rust examples (auto-discovered, excludes zephyr + cross-compiled RTOS)
check:
    #!/usr/bin/env bash
    set -e
    echo "Checking examples..."
    for toml in $(find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' \
                       -not -path '*/build/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' | sort); do
        dir="$(dirname "$toml")"
        platform="$(echo "$dir" | cut -d/ -f2)"
        flags=""
        env_prefix=""
        if [ "$platform" != "native" ]; then flags="--release"; fi
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32-baremetal" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
        fi
        echo "  check $dir"
        (cd "$dir" && cargo +nightly fmt --check && eval $env_prefix cargo clippy $flags -- {{CLIPPY_LINTS}})
    done
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
