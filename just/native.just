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
[group("main")]
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
[group("main")]
build-examples JOBS=env_var_or_default("NROS_BUILD_JOBS", "75%"):
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        cargo_frontends="$(nros_cargo_frontend_jobs)"
    else
        cargo_frontends="{{JOBS}}"
    fi
    echo "Building examples (parallel, cargo-frontends=$cargo_frontends, profile=$(nros_cargo_profile_name))..."
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        echo "NROS_JOBSERVER=1 — serial launcher; cargo inherits fifo tokens."
        SERIAL=1
    elif ! command -v parallel >/dev/null 2>&1; then
        echo "GNU parallel not found — falling back to serial build."
        SERIAL=1
    else
        SERIAL=0
    fi

    toml_list="$(mktemp "${TMPDIR:-/tmp}/nros_example_toml.XXXXXX")"
    trap 'rm -f "$toml_list"' EXIT

    find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' \
                       -not -path '*/build/*' -not -path '*/build-*/*' \
                       -not -path '*/_deps/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-esp32-baremetal/rust/dds/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' \
                       -not -path '*/px4/*' | sort > "$toml_list"

    build_one() {
        local toml="$1"
        local dir="$(dirname "$toml")"
        local platform="$(echo "$dir" | cut -d/ -f2)"
        local env_prefix=""
        local toolchain=""
        if [ "$platform" = "esp32" ] || [ "$platform" = "qemu-esp32-baremetal" ]; then
            env_prefix="SSID=${SSID:-test} PASSWORD=${PASSWORD:-test}"
            toolchain="+{{NIGHTLY}}"
        fi
        echo "  build $dir"
        ( cd "$dir" && eval $env_prefix cargo $toolchain build $cargo_profile_args )
    }
    export -f build_one
    export NIGHTLY="{{NIGHTLY}}"

    if [ "$SERIAL" = "1" ]; then
        while read -r toml; do build_one "$toml"; done < "$toml_list"
    else
        parallel --halt now,fail=1 --line-buffer -j "$cargo_frontends" build_one :::: "$toml_list"
    fi
    echo "All examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
[group("full-matrix")]
build-fixtures: build-fixture-rust build-fixture-extras
    @echo "Native test fixtures built."

# Build every native Rust test fixture from the SSOT manifest
# (examples/fixtures.toml). Per-fixture build options — features, target-dir,
# env, per-RMW variants — live in the manifest; this recipe owns only the
# codegen prep and the build loop, so the build and the test-all staleness
# probe (scripts/test/rust-fixture-stale.sh) consume identical options.
# Phase 181.3.
[group("examples")]
build-fixture-rust:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nros_cli="$(nros_cli_bin)"
    # Codegen the fixtures that consume generated message crates (idempotent —
    # --force is a no-op when the .msg inputs are unchanged). Codegen is a
    # per-platform prep step, so it stays in the recipe; the build loop itself
    # is the shared, manifest-driven scripts/build/fixtures-build.sh.
    for ex in talker listener service-server service-client action-server \
              action-client custom-transport-talker custom-transport-listener; do
        ( cd "examples/native/rust/$ex" && "$nros_cli" generate-rust --force > /dev/null )
    done
    bash scripts/build/fixtures-build.sh native rust
    echo "Native rust fixtures built (from examples/fixtures.toml)."

# Internal Phase 178.B target: only fixture artifacts not already covered by
# `build-fixture-role-examples`.
[group("examples")]
build-fixture-extras:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/build/cmake-incremental.sh
    source scripts/build/fixture-matrix.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    repo_root="$PWD"
    echo "Building native test fixtures..."
    # Phase 176 — cmake parallelism: under the fifo jobserver
    # (NROS_JOBSERVER=1) leave CMAKE_BUILD_PARALLEL_LEVEL unset so the
    # generator (ninja/make) inherits the shared pool; otherwise pin it
    # to the budget (the explicit `--parallel` flag was removed).
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    nros_cli="$(nros_cli_bin)"
    nros_cargo_ensure_codegen_c
    codegen_tool="${repo_root}/$(nros_cargo_codegen_c_bin)"
    ensure_native_rust_generated() {
        case "$1" in
            native/rust/talker|native/rust/listener|native/rust/service-server|native/rust/service-client|native/rust/action-server|native/rust/action-client)
                ( cd "examples/$1" && "$nros_cli" generate-rust --force )
                ;;
        esac
    }
    # Talker is built with `--features param-services` so the params tests
    # see "Counter start value: 0" when the talker reads its declared param.
    # Same binary path as the default-feature talker, just feature-augmented.
    echo "  → native/rust/talker (--features param-services)"
    ensure_native_rust_generated native/rust/talker
    ( cd examples/native/rust/talker && \
      cargo build $cargo_profile_args --features param-services --quiet )
    # Phase 131.B — bench/smoke binaries relocated under packages/testing/.
    test_fixtures=(
        packages/testing/nros-bench/stress-zenoh
        packages/testing/nros-bench/executor-fairness
        packages/testing/nros-bench/stress-xrce
        packages/testing/nros-bench/large-msg-xrce
    )
    for fx in "${test_fixtures[@]}"; do
        echo "  → $fx"
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            ( cd "$fx" && cargo build $cargo_profile_args --quiet )
        else
            ( cd "$fx" && cargo build $cargo_profile_args --quiet ) &
        fi
    done
    wait
    echo "  → native/rust/{talker,listener} (--features link-tls)"
    ensure_native_rust_generated native/rust/talker
    ensure_native_rust_generated native/rust/listener
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        ( cd examples/native/rust/talker && \
          cargo build $cargo_profile_args --features link-tls --target-dir target-tls --quiet )
        ( cd examples/native/rust/listener && \
          cargo build $cargo_profile_args --features link-tls --target-dir target-tls --quiet )
    else
        ( cd examples/native/rust/talker && \
          cargo build $cargo_profile_args --features link-tls --target-dir target-tls --quiet ) &
        ( cd examples/native/rust/listener && \
          cargo build $cargo_profile_args --features link-tls --target-dir target-tls --quiet ) &
    fi
    wait
    echo "  → native/rust/{talker,listener} (--features safety-e2e)"
    ensure_native_rust_generated native/rust/talker
    ensure_native_rust_generated native/rust/listener
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        ( cd examples/native/rust/talker && \
          cargo build $cargo_profile_args --features safety-e2e --target-dir target-safety --quiet )
        ( cd examples/native/rust/listener && \
          cargo build $cargo_profile_args --features safety-e2e --target-dir target-safety --quiet )
    else
        ( cd examples/native/rust/talker && \
          cargo build $cargo_profile_args --features safety-e2e --target-dir target-safety --quiet ) &
        ( cd examples/native/rust/listener && \
          cargo build $cargo_profile_args --features safety-e2e --target-dir target-safety --quiet ) &
    fi
    wait
    echo "  → native/rust/listener (--features unstable-zenoh-api)"
    ensure_native_rust_generated native/rust/listener
    ( cd examples/native/rust/listener && \
      cargo build $cargo_profile_args --features unstable-zenoh-api --target-dir target-zero-copy --quiet )
    # Phase 115.F / 179.G — custom-transport loopback fixtures. These
    # bridge zenoh-pico's `Z_LINK_TYPE_CUSTOM` over a user TcpStream and
    # back the `custom_transport_loopback` integration test. They use the
    # std_msgs binding, so codegen must run before the cargo build.
    for ct_case in custom-transport-talker custom-transport-listener; do
        echo "  → native/rust/${ct_case}"
        ( cd examples/native/rust/${ct_case} && "$nros_cli" generate-rust --force > /dev/null )
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            ( cd examples/native/rust/${ct_case} && cargo build $cargo_profile_args --quiet )
        else
            ( cd examples/native/rust/${ct_case} && cargo build $cargo_profile_args --quiet ) &
        fi
    done
    wait
    # Phase 118.A.2 — collapsed-shape native talker. Built once per RMW
    # feature into isolated target-<rmw>/ dirs so the test harness's
    # `build_native_talker_rmw(Rmw::{Zenoh,Xrce})` resolver finds the
    # matching binary. Param-services is zenoh-only (the gate references
    # nros-rmw-zenoh's optional feature; rmw-xrce builds omit it).
    #
    # Phase 169 retired dust-dds (pure-Rust). Its Cyclone replacement is
    # CMake-linked C++ (`nros_rmw_cyclonedds_register` has no pure-cargo
    # build), so this pure-cargo loop stays zenoh/xrce-only. The native
    # talker/listener Cyclone cells are built by the CMake/Corrosion
    # block below when `just cyclonedds setup` has populated build/install.
    for collapsed_case in $(nros_fixture_roles); do
        if [ -d examples/native/rust/${collapsed_case} ]; then
            ensure_native_rust_generated "native/rust/${collapsed_case}"
            for rmw in zenoh xrce; do
                feature_flags="--features rmw-${rmw}"
                if [ "$rmw" = "zenoh" ] && [ "$collapsed_case" = "talker" ]; then
                    feature_flags="${feature_flags},param-services"
                fi
                echo "  → native/rust/${collapsed_case} (--features rmw-${rmw}, target-${rmw}/)"
                if [ "${NROS_JOBSERVER:-}" = "1" ]; then
                    ( cd examples/native/rust/${collapsed_case} && \
                      cargo build $cargo_profile_args --no-default-features ${feature_flags} \
                          --target-dir target-${rmw} --quiet )
                else
                    ( cd examples/native/rust/${collapsed_case} && \
                      cargo build $cargo_profile_args --no-default-features ${feature_flags} \
                          --target-dir target-${rmw} --quiet ) &
                fi
            done
        fi
    done
    wait
    if [ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]; then
        for collapsed_case in talker listener; do
            echo "  → native/rust/${collapsed_case} (-DNROS_RMW=cyclonedds, build-cyclonedds/)"
            ensure_native_rust_generated "native/rust/${collapsed_case}"
            ( cd "examples/native/rust/${collapsed_case}" && \
              nros_cmake_configure_if_needed . build-cyclonedds -DNROS_RMW=cyclonedds \
                  -DCMAKE_BUILD_TYPE=Release \
                  -DNANO_ROS_BUILD_CODEGEN=OFF \
                  -D_NANO_ROS_CODEGEN_TOOL="${codegen_tool}" \
                  -DCMAKE_PREFIX_PATH="${repo_root}/build/install" > /dev/null && \
              cmake --build build-cyclonedds > /dev/null ) &
        done
        wait
    else
        echo "  → native/rust/{talker,listener} cyclonedds skipped (run: just cyclonedds setup)"
    fi
    # Phase 181.5 — collapsed-shape native C / C++ examples from the SSOT
    # manifest (examples/fixtures.toml). Per-cell RMW (`-DNROS_RMW`),
    # per-RMW build-subdir (`build-<rmw>/`) and any cmake target come from
    # the manifest; the platform-wide defs (build type, the C codegen tool,
    # codegen-off, cyclone prefix) are injected here via the shared builder's
    # NROS_CMAKE_EXTRA_DEFS contract. The canonical typed std_msgs /
    # example_interfaces bindings build cleanly under zenoh + xrce; the
    # cyclone cells stay in the gated block below (need `just cyclonedds
    # setup`). The cpp/parameters cell (target cpp_parameters) is a manifest
    # row too, so it builds in the same pass.
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_BUILD_TYPE=Release -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL=${codegen_tool} -DCMAKE_PREFIX_PATH=${repo_root}/build/install"
    bash scripts/build/fixtures-build.sh native c
    bash scripts/build/fixtures-build.sh native cpp
    unset NROS_CMAKE_EXTRA_DEFS
    if [ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]; then
        for lang in c cpp; do
            for case in talker listener; do
                case "$lang/$case" in
                    c/talker) binary="c_talker" ;;
                    c/listener) binary="c_listener" ;;
                    cpp/talker) binary="cpp_talker" ;;
                    cpp/listener) binary="cpp_listener" ;;
                esac
                echo "  → native/${lang}/${case} (-DNROS_RMW=cyclonedds, build-cyclonedds/)"
                ( cd examples/native/${lang}/${case} && \
                  nros_cmake_configure_if_needed . build-cyclonedds -DNROS_RMW=cyclonedds \
                      -DCMAKE_BUILD_TYPE=Release \
                      -DNANO_ROS_BUILD_CODEGEN=OFF \
                      -D_NANO_ROS_CODEGEN_TOOL="${codegen_tool}" \
                      -DCMAKE_PREFIX_PATH="${repo_root}/build/install" \
                      -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="${repo_root}/scripts/cyclonedds/msg_to_cyclone_idl.py" > /dev/null && \
                  cmake --build build-cyclonedds --target "$binary" > /dev/null ) &
            done
        done
        wait
    else
        echo "  → native c/cpp cyclonedds skipped (run: just cyclonedds setup)"
    fi
    echo "  → packages/testing/nros-bench/stress-zenoh (ZPICO_SUBSCRIBER_BUFFER_SIZE=8192)"
    ( cd packages/testing/nros-bench/stress-zenoh && \
      ZPICO_SUBSCRIBER_BUFFER_SIZE=8192 cargo build $cargo_profile_args --target-dir target-large-buf --quiet )

# Build every native tier: core artifacts, Rust/C/C++ examples, and test fixtures.
[group("full-matrix")]
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

    toml_list="$(mktemp "${TMPDIR:-/tmp}/nros_check_toml.XXXXXX")"
    trap 'rm -f "$toml_list"' EXIT

    find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' \
                       -not -path '*/build/*' -not -path '*/build-*/*' \
                       -not -path '*/zephyr/*' \
                       -not -path '*/multi-package-workspace/*' \
                       -not -path '*/qemu-esp32-baremetal/rust/dds/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' \
                       -not -path '*/px4/*' | sort > "$toml_list"

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
        while read -r toml; do check_one "$toml"; done < "$toml_list"
    else
        parallel --halt now,fail=1 --line-buffer -j {{JOBS}} check_one :::: "$toml_list"
    fi
    echo "All examples check passed!"

# Show embedded example binary sizes
size: build-examples
    @echo ""
    @echo "Binary sizes (release):"
    @echo "======================="
    @size packages/reference/stm32f4-porting/rtic/target/thumbv7em-none-eabihf/release/stm32f4-rs-rtic-example 2>/dev/null || echo "RTIC: build failed"
    @size examples/stm32f4/rust/talker-embassy/target/thumbv7em-none-eabihf/release/stm32f4-rs-embassy-example 2>/dev/null || echo "Embassy: build failed"
    @size packages/reference/stm32f4-porting/polling/target/thumbv7em-none-eabihf/release/stm32f4-rs-polling-example 2>/dev/null || echo "Polling: build failed"
    @size packages/testing/nros-smoke/stm32f4-smoltcp-echo/target/thumbv7em-none-eabihf/release/stm32f4-rs-smoltcp 2>/dev/null || echo "stm32f4-smoltcp-echo: build failed"

# Clean native example build artifacts.
[group("main")]
clean:
    #!/usr/bin/env bash
    find examples/native -type d \( -name target -o -name build -o -name 'build-*' \) \
        -prune -exec rm -rf {} + 2>/dev/null || true
    echo "Native example build artifacts cleaned"

# =============================================================================
# Native C/C++ examples (CMake-based)
# =============================================================================

# Build a single C example with CMake (Phase 140 — add_subdirectory shape).
[no-exit-message]
_build-c-example dir *CMAKE_ARGS:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/build/cmake-incremental.sh
    echo "Building {{dir}}..."
    nros_cargo_ensure_codegen_c
    codegen_tool="$(pwd)/$(nros_cargo_codegen_c_bin)"
    nros_cmake_configure_if_needed "{{dir}}" "{{dir}}/build" \
        -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=zenoh -DNROS_RMW=zenoh \
        -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" {{CMAKE_ARGS}}
    cmake --build "{{dir}}/build"

# Build C examples only (no tests)
[group("examples")]
build-c:
    #!/usr/bin/env bash
    set -e
    just native _build-c-example examples/native/c/talker
    just native _build-c-example examples/native/c/listener
    just native _build-c-example examples/native/c/custom-msg
    echo "C examples built!"

# Build C XRCE examples only (no tests)
[group("examples")]
build-c-xrce:
    #!/usr/bin/env bash
    set -e
    just native _build-c-example examples/native/c/talker  "-DNANO_ROS_RMW=xrce -DNROS_RMW=xrce"
    just native _build-c-example examples/native/c/listener "-DNANO_ROS_RMW=xrce -DNROS_RMW=xrce"
    echo "C XRCE examples built!"

# Clean C examples build
[group("main")]
clean-c:
    find examples/native/c -type d \( -name build -o -name 'build-*' \) \
        -prune -exec rm -rf {} + 2>/dev/null || true
    @echo "C examples build cleaned"

# Build a single C++ example with CMake (Phase 140 — add_subdirectory shape).
[no-exit-message]
_build-cpp-example dir *CMAKE_ARGS:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/build/cmake-incremental.sh
    echo "Building {{dir}}..."
    nros_cargo_ensure_codegen_c
    codegen_tool="$(pwd)/$(nros_cargo_codegen_c_bin)"
    nros_cmake_configure_if_needed "{{dir}}" "{{dir}}/build" \
        -DNANO_ROS_PLATFORM=posix -DNANO_ROS_RMW=zenoh -DNROS_RMW=zenoh \
        -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" {{CMAKE_ARGS}}
    cmake --build "{{dir}}/build"

# Build C++ examples only (no tests)
[group("examples")]
build-cpp:
    #!/usr/bin/env bash
    set -e
    just native _build-cpp-example examples/native/cpp/talker
    just native _build-cpp-example examples/native/cpp/listener
    just native _build-cpp-example examples/native/cpp/service-server
    just native _build-cpp-example examples/native/cpp/service-client
    just native _build-cpp-example examples/native/cpp/action-server
    just native _build-cpp-example examples/native/cpp/action-client
    echo "C++ examples built!"

# Clean C++ examples build
[group("main")]
clean-cpp:
    find examples/native/cpp -type d \( -name build -o -name 'build-*' \) \
        -prune -exec rm -rf {} + 2>/dev/null || true
    @echo "C++ examples build cleaned"

# =============================================================================
# Integration tests
# =============================================================================

# Run integration tests only (requires zenohd). Focused ROS 2, large-msg,
# native C/C++, and XRCE C API lanes live below; this default native tier
# keeps those slower slices out of the broad local loop.
[group("main")]
test verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    # Drop ROS 2 / XRCE-interop binaries by their nextest test-groups
    # (`.config/nextest.toml`) so new interop binaries assigned to those
    # groups are excluded automatically — no per-binary chain to maintain.
    # The remaining explicit `binary(...)` excludes are carve-outs with
    # their own focused lane and no shared group: zephyr (native_sim
    # fixtures), esp32_emulator, large_msg (test-large-msg), and the
    # native_api / cpp_parameters / c_xrce_api fixture lanes
    # (test-native-api, test-c-xrce). Unlike root `just test`, this lane
    # keeps the QEMU RTOS e2e groups in.
    args=(-p nros-tests --no-fail-fast \
          -E 'not (group(=ros2-interop) or group(=xrce_ros2_interop) or binary(zephyr) or binary(esp32_emulator) or binary(large_msg) or binary(native_api) or binary(cpp_parameters) or binary(c_xrce_api))')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run all native tests (integration + ROS 2 interop + large_msg + C/C++ API)
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just native test {{verbose}} || failed=1
    just native test-large-msg {{verbose}} || failed=1
    just native test-ros2 {{verbose}} || failed=1
    just native test-ros2-params {{verbose}} || failed=1
    just native test-ros2-lifecycle {{verbose}} || failed=1
    just native test-native-api {{verbose}} || failed=1
    just native test-c-xrce {{verbose}} || failed=1
    just native _test-c-codegen {{verbose}} || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some native tests failed."
        exit 1
    fi

# CI: check + test
[group("full-matrix")]
ci: check test

# Run large message & throughput stress tests
[group("debug")]
test-large-msg verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test large_msg --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run RMW abstraction integration tests
[group("debug")]
test-rmw verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test rmw --features rmw --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run ROS 2 interop tests (Rust test harness)
[group("debug")]
test-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test rmw_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run ROS 2 parameter interop tests.
# Requires `just rmw_zenoh setup` first so rmw_zenoh_cpp matches our zenoh-pico pin.
[group("debug")]
test-ros2-params verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test params --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run the `ros2 lifecycle …` interop test against an nros lifecycle-node.
# Requires `just rmw_zenoh setup` first so rmw_zenoh_cpp matches our zenoh-pico pin.
[group("debug")]
test-ros2-lifecycle verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test ros2_lifecycle_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run ROS 2 interop tests (shell script)
[group("debug")]
test-ros2-shell:
    ./tests/ros2-interop.sh

# Run DDS ROS 2 interop tests (requires ROS 2 + rmw_cyclonedds_cpp)
[group("debug")]
test-dds-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test dds_ros2_interop --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

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

# Run the full native C/C++ API nextest lane.
[group("debug")]
test-native-api verbose="":
    #!/usr/bin/env bash
    set -e
    # Consumes prebuilt per-RMW CMake fixtures; run
    # `just build-test-fixtures` first for the full matrix.
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --test native_api --test cpp_parameters --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run all C tests (integration + codegen)
[group("debug")]
test-c verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    just init-test-logs
    args=(-p nros-tests --test native_api --no-fail-fast \
          -E 'test(/Language__C$/) or test(test_c_)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    just native _test-c-codegen {{verbose}}

# C++ API integration tests (needs cmake + zenohd)
[group("debug")]
test-cpp verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    just build-zenohd
    args=(-p nros-tests --no-fail-fast \
          -E '(binary(native_api) and (test(Language__Cpp) or test(test_cpp_))) or binary(cpp_parameters)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# C XRCE-DDS API integration tests (needs cmake + XRCE Agent)
[group("debug")]
test-c-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --no-fail-fast -E 'binary(c_xrce_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Run executor fairness benchmark (requires zenohd)
bench-fairness:
    cd packages/testing/nros-bench/executor-fairness && \
        RUST_LOG=warn cargo run --release

# =============================================================================
# Run examples
# =============================================================================

# Run native Rust talker
talker:
    cd examples/native/rust/talker && cargo run

# Run native Rust listener
listener:
    cd examples/native/rust/listener && cargo run

# Run native Rust service server
service-server:
    cd examples/native/rust/service-server && cargo run

# Run native Rust service client
service-client:
    cd examples/native/rust/service-client && cargo run

# Run native Rust action server
action-server:
    cd examples/native/rust/action-server && cargo run

# Run native Rust action client
action-client:
    cd examples/native/rust/action-client && cargo run

# Start zenohd router for native examples (port 7447)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7447 --no-multicast-scouting
