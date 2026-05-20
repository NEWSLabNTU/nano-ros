set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# CLIPPY_LINTS removed — workspace-wide lint config now lives in root Cargo.toml `[workspace.lints]`

LOG_DIR := "test-logs"
NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# =============================================================================
# Rust examples (auto-discovered from examples/{platform}/{lang}/{rmw}/{use-case}/)
# =============================================================================

# Build native core artifacts.
build:
    @echo "Native example module has no separate core build; use 'just native build-examples' for examples."

# Build all Rust examples (auto-discovered, excludes zephyr + cross-compiled RTOS).
#
# Parallelised via GNU parallel — examples are independent standalone
# crates (per the copy-out invariant in CLAUDE.md), so each cargo
# invocation can run concurrently. JOBS defaults to the global
# NROS_BUILD_JOBS budget (itself nproc unless overridden), falling back
# to 75 % of cores. Override per-invocation with
# `just native build-examples JOBS=4` for tighter machines.
build-examples JOBS=env_var_or_default("NROS_BUILD_JOBS", "75%"):
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
                       -not -path '*/build/*' -not -path '*/build-*/*' \
                       -not -path '*/_deps/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-esp32-baremetal/rust/dds/*' \
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
    # Phase 176 — cmake parallelism: under the fifo jobserver
    # (NROS_JOBSERVER=1) leave CMAKE_BUILD_PARALLEL_LEVEL unset so the
    # generator (ninja/make) inherits the shared pool; otherwise pin it
    # to the budget (the explicit `--parallel` flag was removed).
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
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
        native/rust/zenoh/talker-rtic
        native/rust/zenoh/listener-rtic
        native/rust/zenoh/service-server-rtic
        native/rust/zenoh/service-client-rtic
        native/rust/zenoh/action-server-rtic
        native/rust/zenoh/action-client-rtic
        native/rust/xrce/talker
        native/rust/xrce/listener
        native/rust/xrce/service-server
        native/rust/xrce/service-client
        native/rust/xrce/action-server
        native/rust/xrce/action-client
        native/rust/xrce/serial-talker
        native/rust/xrce/serial-listener
    )
    for ex in "${examples[@]}"; do
        echo "  → $ex"
        ( cd "examples/$ex" && cargo build --release --quiet ) &
    done
    # Phase 131.B — bench/smoke binaries relocated under packages/testing/.
    test_fixtures=(
        packages/testing/nros-bench/stress-zenoh
        packages/testing/nros-bench/executor-fairness
        packages/testing/nros-bench/stress-xrce
        packages/testing/nros-bench/large-msg-xrce
    )
    for fx in "${test_fixtures[@]}"; do
        echo "  → $fx"
        ( cd "$fx" && cargo build --release --quiet ) &
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
    # Phase 118.A.2 — collapsed-shape native talker. Built once per RMW
    # feature into isolated target-<rmw>/ dirs so the test harness's
    # `build_native_talker_rmw(Rmw::{Zenoh,Xrce})` resolver finds the
    # matching binary. Param-services is zenoh-only (the gate references
    # nros-rmw-zenoh's optional feature; rmw-xrce builds omit it).
    #
    # Phase 169 retired dust-dds (pure-Rust). Its Cyclone replacement is
    # CMake-linked C++ (`nros_rmw_cyclonedds_register` has no pure-cargo
    # build), so the migrated `rmw-cyclonedds` feature stays defined in
    # each Cargo.toml but is NOT built here — Cyclone-for-cargo-Rust
    # examples are deferred to their own phase (CMake/Corrosion glue for
    # native + a ddsrt RTOS port for embedded). See
    # docs/roadmap/phase-175-cyclone-rust-example-build-path.md.
    for collapsed_case in talker listener service-server service-client action-server action-client; do
        if [ -d examples/native/rust/${collapsed_case} ]; then
            for rmw in zenoh xrce; do
                feature_flags="--features rmw-${rmw}"
                if [ "$rmw" = "zenoh" ] && [ "$collapsed_case" = "talker" ]; then
                    feature_flags="${feature_flags},param-services"
                fi
                echo "  → native/rust/${collapsed_case} (--features rmw-${rmw}, target-${rmw}/)"
                ( cd examples/native/rust/${collapsed_case} && \
                  cargo build --release --no-default-features ${feature_flags} \
                      --target-dir target-${rmw} --quiet ) &
            done
        fi
    done
    wait
    # Phase 118.A.3 / 118.B.2 — collapsed-shape native C examples. Each
    # case built once per RMW via cmake `-DNROS_RMW=<rmw>` into
    # isolated build-<rmw>/ dirs. The canonical zenoh-style main.c
    # uses the typed std_msgs / example_interfaces binding and builds
    # cleanly under all three RMWs (zenoh, dds, xrce); the legacy
    # `c/xrce/<case>/` siblings' manual-CDR variant is now redundant
    # and is dropped by Tier 5 cleanup.
    for case in talker listener service-server service-client action-server action-client; do
        if [ -d examples/native/c/${case} ]; then
            for rmw in zenoh xrce; do
                echo "  → native/c/${case} (-DNROS_RMW=${rmw}, build-${rmw}/)"
                ( cd examples/native/c/${case} && \
                  cmake -B build-${rmw} -S . -DNROS_RMW=${rmw} -DCMAKE_BUILD_TYPE=Release > /dev/null && \
                  cmake --build build-${rmw} > /dev/null ) &
            done
        fi
    done
    wait
    # Phase 118.B.3 — collapsed-shape native C++ examples. Same shape
    # as the C side; the typed nros-cpp binding handles all three RMWs.
    for case in talker listener service-server service-client action-server action-client; do
        if [ -d examples/native/cpp/${case} ]; then
            for rmw in zenoh xrce; do
                echo "  → native/cpp/${case} (-DNROS_RMW=${rmw}, build-${rmw}/)"
                ( cd examples/native/cpp/${case} && \
                  cmake -B build-${rmw} -S . -DNROS_RMW=${rmw} -DCMAKE_BUILD_TYPE=Release > /dev/null && \
                  cmake --build build-${rmw} > /dev/null ) &
            done
        fi
    done
    wait
    echo "  → packages/testing/nros-bench/stress-zenoh (ZPICO_SUBSCRIBER_BUFFER_SIZE=8192)"
    ( cd packages/testing/nros-bench/stress-zenoh && \
      ZPICO_SUBSCRIBER_BUFFER_SIZE=8192 cargo build --release --target-dir target-large-buf --quiet )

# Build every native tier: core artifacts, Rust/C/C++ examples, and test fixtures.
build-all: build build-examples build-c build-c-xrce build-cpp build-fixtures
    @echo "Native build-all completed."

# Format all Rust examples (auto-discovered, parallel)
format:
    #!/usr/bin/env bash
    set -e
    echo "Formatting examples..."
    find examples -mindepth 4 -name Cargo.toml -not -path '*/target/*' \
        -not -path '*/build/*' -not -path '*/build-*/*' \
        -not -path '*/generated/*' -not -path '*/zephyr/*' \
        -not -path '*/multi-package-workspace/*' \
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
                       -not -path '*/multi-package-workspace/*' \
                       -not -path '*/qemu-esp32-baremetal/rust/dds/*' \
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
size: build-examples
    @echo ""
    @echo "Binary sizes (release):"
    @echo "======================="
    @size packages/reference/stm32f4-porting/rtic/target/thumbv7em-none-eabihf/release/stm32f4-rs-rtic-example 2>/dev/null || echo "RTIC: build failed"
    @size examples/stm32f4/rust/zenoh/talker-embassy/target/thumbv7em-none-eabihf/release/stm32f4-rs-embassy-example 2>/dev/null || echo "Embassy: build failed"
    @size packages/reference/stm32f4-porting/polling/target/thumbv7em-none-eabihf/release/stm32f4-rs-polling-example 2>/dev/null || echo "Polling: build failed"
    @size packages/testing/nros-smoke/stm32f4-smoltcp-echo/target/thumbv7em-none-eabihf/release/stm32f4-rs-smoltcp 2>/dev/null || echo "stm32f4-smoltcp-echo: build failed"

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

# Build a single C example with CMake (Phase 140 — add_subdirectory shape).
[no-exit-message]
_build-c-example dir *CMAKE_ARGS:
    #!/usr/bin/env bash
    set -e
    echo "Building {{dir}}..."
    cd "{{dir}}" && rm -rf build && mkdir -p build && cd build
    cmake -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=zenoh {{CMAKE_ARGS}} ..
    make

# Build C examples only (no tests)
build-c:
    #!/usr/bin/env bash
    set -e
    just native _build-c-example examples/native/c/zenoh/talker
    just native _build-c-example examples/native/c/zenoh/listener
    just native _build-c-example examples/native/c/zenoh/custom-msg
    echo "C examples built!"

# Build C XRCE examples only (no tests)
build-c-xrce:
    #!/usr/bin/env bash
    set -e
    just native _build-c-example examples/native/c/xrce/talker  "-DNANO_ROS_RMW=xrce"
    just native _build-c-example examples/native/c/xrce/listener "-DNANO_ROS_RMW=xrce"
    echo "C XRCE examples built!"

# Clean C examples build
clean-c:
    rm -rf examples/native/c/zenoh/{talker,listener,custom-msg}/build
    rm -rf examples/native/c/xrce/{talker,listener}/build
    @echo "C examples build cleaned"

# Build a single C++ example with CMake (Phase 140 — add_subdirectory shape).
[no-exit-message]
_build-cpp-example dir *CMAKE_ARGS:
    #!/usr/bin/env bash
    set -e
    echo "Building {{dir}}..."
    cd "{{dir}}" && rm -rf build && mkdir -p build && cd build
    cmake -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=zenoh {{CMAKE_ARGS}} ..
    make

# Build C++ examples only (no tests)
build-cpp:
    #!/usr/bin/env bash
    set -e
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

# Phase 126 coverage — promote the orchestration E2E fixtures from
# the codegen submodule into the top-level `just test-all` matrix.
# The tests live in `packages/codegen/packages/nros-cli-core/tests/
# orchestration_e2e.rs` and drive metadata → plan → check → build →
# native run (and FreeRTOS QEMU + mixed-C + services/actions) of a
# fixture workspace. They depend on `build/zenohd/zenohd` for the
# native spin-up + `arm-none-eabi-gcc` / `qemu-system-arm` for the
# FreeRTOS variant; each #[test] gates itself when the toolchain
# is absent.
_test-orchestration-e2e verbose="":
    #!/usr/bin/env bash
    set -e
    just init-test-logs
    v="{{ if verbose != "" { "--verbose" } else { "" } }}"
    ./tests/run-test.sh --name orchestration-e2e \
        --log {{LOG_DIR}}/latest/orchestration-e2e.log $v -- \
        bash -c 'cd packages/codegen/packages && cargo test -p nros-cli-core --test orchestration_e2e -- --nocapture'

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

# C++ API integration tests (needs cmake + zenohd)
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
    cd packages/testing/nros-bench/executor-fairness && \
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
