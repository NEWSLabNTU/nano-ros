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

    # Phase 181.6 / 181.4.h — qemu-arm-baremetal, stm32f4, esp32 and
    # qemu-esp32-baremetal are excluded here and built from the SSOT manifest
    # below instead, so each has a single enumeration source (matching the
    # test-all staleness probe). The remaining platforms have no manifest rows
    # for this broad find, or are built by their own platform recipes
    # (freertos/nuttx/threadx/zephyr/px4).
    find examples -mindepth 4 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' -not -path '*/build/*' \
                       -not -path '*/build/*' -not -path '*/build-*/*' \
                       -not -path '*/_deps/*' -not -path '*/zephyr/*' \
                       -not -path '*/qemu-arm-freertos/*' -not -path '*/qemu-arm-nuttx/*' \
                       -not -path '*/threadx-linux/*' \
                       -not -path '*/qemu-riscv64-threadx/*' \
                       -not -path '*/qemu-arm-baremetal/*' -not -path '*/stm32f4/*' \
                       -not -path '*/esp32/*' -not -path '*/qemu-esp32-baremetal/*' \
                       -not -path '*/px4/*' | sort > "$toml_list"

    # The ESP platforms (which need the nightly toolchain + SSID/PASSWORD env)
    # are excluded from this find and built from the manifest below, so this
    # loop only sees host examples — a plain `cargo build` suffices.
    build_one() {
        local toml="$1"
        local dir="$(dirname "$toml")"
        echo "  build $dir"
        ( cd "$dir" && cargo build $cargo_profile_args )
    }
    export -f build_one

    if [ "$SERIAL" = "1" ]; then
        while read -r toml; do build_one "$toml"; done < "$toml_list"
    else
        parallel --halt now,fail=1 --line-buffer -j "$cargo_frontends" build_one :::: "$toml_list"
    fi
    # Phase 181.6 — qemu-arm-baremetal + stm32f4 rust examples from the SSOT
    # manifest (plain default builds; each example's .cargo/config.toml selects
    # the cross target). No codegen prep needed (no generated-msg consumers).
    bash scripts/build/fixtures-build.sh qemu-arm-baremetal rust
    bash scripts/build/fixtures-build.sh stm32f4 rust
    # Phase 181.4.h — esp32 + qemu-esp32-baremetal (ESP32-C3, riscv32imc) from
    # the manifest. They need the workspace nightly + rust-src/build-std (NOT
    # xtensa/espup) + SSID/PASSWORD; inject those as platform env.
    RUSTUP_TOOLCHAIN="{{NIGHTLY}}" SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" \
        bash scripts/build/fixtures-build.sh esp32 rust
    RUSTUP_TOOLCHAIN="{{NIGHTLY}}" SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" \
        bash scripts/build/fixtures-build.sh qemu-esp32-baremetal rust
    echo "All examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++ + workspaces).
[group("full-matrix")]
build-fixtures: build-fixture-rust build-fixture-extras build-workspace-fixtures
    @echo "Native test fixtures built."

# Build native workspace fixtures from examples/fixtures.toml.
[group("examples")]
build-workspace-fixtures:
    bash scripts/build/workspace-fixtures-build.sh native

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
    # Phase 214.J.2 — codegen-stamp guard for trait-surface drift.
    source scripts/build/codegen-stamp.sh
    nros_cli="$(nros_cli_bin)"
    # Phase 214.I.2 — fail-loud prereq guard: the loop below invokes
    # `nros ws sync`, absent from the shipped 0.3.7 release. Probe once;
    # emit `[PREREQ]` + exit 0 if missing rather than cascading into
    # clap "unrecognized subcommand 'ws'" stack traces.
    nros_require_ws_sync "$nros_cli"
    # Phase 210.E.3.d.native — pre-cargo codegen via `nros ws sync` writes
    # the auto-managed [patch.crates-io] block into each example's Cargo.toml
    # + materialises generated msg crates under `<ex>/generated/`.
    # Replaces the legacy `nros generate-rust --force` + hand-managed
    # `.cargo/config.toml [patch.crates-io]` chunks.
    nros_repo_root="$PWD"
    for ex in talker listener service-server service-client action-server \
              action-client custom-transport-talker custom-transport-listener; do
        dir="examples/native/rust/$ex"
        NROS_REPO_DIR="$nros_repo_root" nros_codegen_stamp_check_or_wipe "$dir"
        NROS_REPO_DIR="$nros_repo_root" "$nros_cli" ws sync "$dir" >/dev/null
        NROS_REPO_DIR="$nros_repo_root" nros_codegen_stamp_write "$dir"
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
    # Phase 214.J.2 — codegen-stamp guard for trait-surface drift.
    source scripts/build/codegen-stamp.sh
    source scripts/build/cmake-incremental.sh
    repo_root="$PWD"
    echo "Building native C/C++ + Cyclone test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    nros_cli="$(nros_cli_bin)"
    # Phase 214.I.2 — fail-loud prereq guard: `ensure_native_rust_generated`
    # below invokes `nros ws sync`. Probe once; emit `[PREREQ]` + exit 0
    # if the verb is missing (shipped 0.3.7 predates it).
    nros_require_ws_sync "$nros_cli"
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    # Phase 181.6 — every native *Rust* fixture (the param-services talker; the
    # link-tls / safety-e2e / unstable-zenoh-api variants; the per-RMW
    # target-<rmw>/ collapsed roles; the custom-transport pair; the nros-bench
    # binaries; the large-buf stress build) is now a row in
    # examples/fixtures.toml and built by `build-fixture-rust`. This recipe owns
    # only what the manifest can't express: the gated CMake/Corrosion Cyclone
    # cells (no pure-cargo register symbol) and the C/C++ manifest passes.
    ensure_native_rust_generated() {
        case "$1" in
            native/rust/talker|native/rust/listener)
                # Phase 210.E.3.d.native — same swap as build-fixture-role-
                # examples: `ws sync` for the patch-table + codegen flow.
                # Phase 214.J.2 — also wipe `generated/` if trait surface
                # has drifted since the previous sync.
                NROS_REPO_DIR="$repo_root" nros_codegen_stamp_check_or_wipe "examples/$1"
                NROS_REPO_DIR="$repo_root" "$nros_cli" ws sync "examples/$1" >/dev/null
                NROS_REPO_DIR="$repo_root" nros_codegen_stamp_write "examples/$1" ;;
        esac
    }
    # Phase 220.C (Path B) — native talker/listener Cyclone cells now build via
    # pure-cargo, matching K.7.7 (`fcbc498cc`). The Phase 175.A cmake/corrosion
    # bridge was retired by Phase 212.M.1 (deleted the per-example
    # CMakeLists.txt) once `cargo build --features rmw-cyclonedds` could link
    # `nros_rmw_cyclonedds_register` through `nros-rmw-cyclonedds-sys`. Each
    # example carries an `rmw-cyclonedds = ["nros/rmw-cyclonedds"]` feature
    # that routes the typed-creator hook (K.7.6.b) into the C++ backend.
    for collapsed_case in talker listener; do
        echo "  → native/rust/${collapsed_case} (cargo --features rmw-cyclonedds, target-cyclonedds/)"
        ensure_native_rust_generated "native/rust/${collapsed_case}"
    done
    export NROS_CARGO_PROFILE_ARGS="$cargo_profile_args"
    bash scripts/build/fixture-make-driver.sh native-cyclonedds-rust
    # Phase 181.5 — collapsed-shape native C / C++ examples from the SSOT
    # manifest (examples/fixtures.toml). Per-cell RMW (`-DNROS_RMW`),
    # per-RMW build-subdir (`build-<rmw>/`) and any cmake target come from
    # the manifest; the platform-wide defs (build type, the C codegen tool,
    # codegen-off, cyclone prefix) are injected here via the shared builder's
    # NROS_CMAKE_EXTRA_DEFS contract. The cpp/parameters cell (target
    # cpp_parameters) is a manifest row too, so it builds in the same pass.
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_BUILD_TYPE=Release -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL=${codegen_tool} -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON"
    bash scripts/build/fixture-make-driver.sh native-cmake-rmw
    unset NROS_CMAKE_EXTRA_DEFS
    # Phase 186 — native c/cpp Cyclone cells self-provision from source (static
    # ddsc via nano-ros-posix.cmake; DISABLE_FIND_PACKAGE forces the pinned source
    # over any system Cyclone). No build/install / `just cyclonedds setup` needed.
    # Phase 177.31 fixed the example link gap; the Cyclone *action* e2e is still
    # blocked on a runtime bug (177.32) so its tests skip at runtime, but the
    # fixtures build cleanly here. These cells are manifest rows and run through
    # fixtures-build.sh, preserving the incremental build-cyclonedds directories.
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_BUILD_TYPE=Release -DNANO_ROS_BUILD_CODEGEN=OFF -D_NANO_ROS_CODEGEN_TOOL=${codegen_tool} -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=${repo_root}/scripts/cyclonedds/msg_to_cyclone_idl.py"
    bash scripts/build/fixture-make-driver.sh native-cyclonedds-cmake
    unset NROS_CMAKE_EXTRA_DEFS

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
                       -not -path '*/target/*' -not -path '*/generated/*' -not -path '*/build/*' \
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
        # Quiet by default: no per-example label, and cargo --quiet suppresses
        # the "Checking …" progress + the "Blocking waiting for file lock on
        # package cache" spam (every parallel job contends the shared ~/.cargo
        # package-cache lock). A failing clippy still prints its diagnostic with
        # the example's file path, so failures stay identifiable. Set
        # NROS_CHECK_VERBOSE=1 to restore the per-example progress label.
        if [ -n "${NROS_CHECK_VERBOSE:-}" ]; then echo "  check $dir"; fi
        ( cd "$dir" && cargo +{{NIGHTLY}} fmt --check && eval $env_prefix cargo clippy --quiet $flags )
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
    codegen_tool="$(nros_cargo_codegen_c_bin)"
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
    codegen_tool="$(nros_cargo_codegen_c_bin)"
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
#
# Phase 214.Q: `build-fixtures` is sequenced as a prereq so the nextest
# harness never explodes with `nros_tests::fixtures::binaries::*` "Test
# fixture binary not prebuilt" errors. The harness fails loud on missing
# fixtures by design (Phase 181) — recipes own the build-then-test
# sequencing.
[group("main")]
test verbose="": build-fixtures
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
          -E 'not (group(=ros2-interop) or binary(xrce_ros2_interop) or binary(zephyr) or binary(esp32_emulator) or binary(large_msg) or binary(native_api) or binary(cpp_parameters) or binary(c_xrce_api))')
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

# Run the in-tree C msg-gen test (shell-based, no nextest). Phase 195.D — the
# nros-cli codegen-engine tests (cargo-nano-ros / nros-cli-core) moved out with
# the retired `packages/codegen` submodule; they run in nros-cli's own CI.
_test-c-codegen verbose="":
    #!/usr/bin/env bash
    set -e
    just init-test-logs
    v="{{ if verbose != "" { "--verbose" } else { "" } }}"
    ./tests/run-test.sh --name c-msg-gen --log {{LOG_DIR}}/latest/c-msg-gen.log $v -- ./tests/c-msg-gen-tests.sh

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

# Start zenohd router for native examples (override bind via $ZENOH_LOCATOR)
zenohd:
    zenohd --listen {{ env_var_or_default("ZENOH_LOCATOR", "tcp/127.0.0.1:7447") }} --no-multicast-scouting
