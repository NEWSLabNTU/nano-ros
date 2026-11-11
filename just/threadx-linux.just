set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX Linux prerequisites. Phase 197.4 — `nros setup <board>`
# provisions the threadx + netxduo sources (+ tools) from the SDK index; the
# host-env helper handles rustup. Host build, so no cross target.
# Does NOT build example binaries — use `just threadx_linux build-examples` for that.
[group("setup")]
setup:
    #!/usr/bin/env bash
    # Every RMW this platform's lane BUILDS, not just zenoh.
    #
    # `nros setup <board>` takes `--rmw`'s zenoh default, so a lane that also
    # builds a cyclonedds coordinate never got its host tools — the freertos
    # nightly died at cmake configure with `idlc (Cyclone DDS IDL compiler, a
    # host tool) not found`. `[rmw.cyclonedds]` declares exactly what that
    # needs and calls itself "a complete provisioner"; nothing requested it.
    #
    # The backend list is DERIVED from `examples/fixtures.toml`, not written
    # here, so it cannot drift from the coordinates the lane actually builds.
    # A SHEBANG recipe: `just` runs each line of a line-based recipe in its
    # own shell, so a sourced helper does not survive to the next line — and
    # the default shell is `sh`, which has no `source` at all.
    set -euo pipefail
    source scripts/build/platform-rmws.sh
    nros_setup_board_rmws threadx-linux threadx-linux
    "{{justfile_directory()}}/tools/host-env.sh" threadx

# Diagnose ThreadX Linux install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh threadx_linux doctor "just doctor threadx_linux"
    set +e
    source scripts/build/cargo.sh
    nros_locate_sdk_trees  # phase-484 W2 — the trees W3c stopped exporting
    fail=0
    if [ -d "$THREADX_DIR/common/inc" ]; then
        echo "  [OK] ThreadX kernel: $THREADX_DIR"
    else
        echo "  [MISSING] ThreadX kernel at $THREADX_DIR (run: just threadx_linux setup)"
        fail=1
    fi
    if [ -d "$NETX_DIR/common/inc" ]; then
        echo "  [OK] NetX Duo: $NETX_DIR"
    else
        echo "  [MISSING] NetX Duo at $NETX_DIR (run: just threadx_linux setup)"
        fail=1
    fi
    exit $fail

# Phase 140 — `install-platform` + `install` recipes deleted. ThreadX
# Linux examples consume nano-ros via `add_subdirectory(<repo-root>)`
# (Phase 144.8) — no install prefix needed.

# Build ThreadX Linux examples (canonical "did the core link clean?" smoke).
[group("main")]
build: build-examples
    @echo "ThreadX Linux examples built."

# Build ThreadX Linux examples
[group("main")]
build-examples:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nros_locate_sdk_trees  # phase-484 W2 — the trees W3c stopped exporting
    # issue 0650 — first step of the lane: clear last run's skip notes, so a
    # lane that is complete this time is not reported SKIPPED from history.
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    nros_lane_skip_reset threadx-linux
    source scripts/build/cargo.sh
    # issue 1038 — the central `nros-patch.toml` this lane's Rust leaves
    # `include`, made EXPLICIT. This lane already got the file, but only as a
    # side effect of `workspace-fixtures-build.sh` running `nros configure` somewhere
    # earlier; `threadx-riscv64` had no such neighbour and died at manifest
    # parse. An inherited artifact is a working lane, not a declared dependency.
    # Idempotent — a process spawn when the file is already current.
    nros_ensure_central_patch
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        nros_lane_skip_note threadx-linux "Skipping ThreadX Linux examples (ThreadX/NetX not found). Run: just threadx-linux setup"
        exit 0
    fi
    echo "Building ThreadX Linux examples..."
    # Build options from the SSOT manifest (examples/fixtures.toml). Phase 181.4.f.
    bash scripts/build/fixtures-build.sh threadx-linux rust
    bash scripts/build/workspace-fixtures-build.sh threadx-linux rust
    # phase-263 C2a — the embedded C workspace LAUNCH entry (issue 0097), consumed
    # by tests/entry_e2e.rs (threadx_linux_c cell). Builds into its own build-subdir (one board
    # per CMake configure), so it never clashes with the posix workspace fixtures.
    bash scripts/build/workspace-fixtures-build.sh threadx-linux c
    # phase-263 C2c — the embedded C++ workspace LAUNCH entry, consumed by
    # tests/entry_e2e.rs (threadx_linux_cpp cell).
    bash scripts/build/workspace-fixtures-build.sh threadx-linux cpp
    # phase-263 C2c — the embedded MIXED (C+C++/Rust) workspace LAUNCH entry, consumed
    # by tests/entry_e2e.rs (threadx_linux_mixed cell).
    bash scripts/build/workspace-fixtures-build.sh threadx-linux mixed
    echo "ThreadX Linux examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
[group("full-matrix")]
build-fixtures: build-examples build-fixture-extras
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh threadx_linux build-fixtures "just build threadx_linux"
    set -e
    # issue 0650 — the ONE place this lane says it built its fixtures, and it
    # refuses to when a step was skipped. Before this the claim was an
    # unconditional echo, so a lane with no toolchain printed it anyway.
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    nros_lane_skip_flush threadx-linux "ThreadX-Linux test fixtures built."

# Internal Phase 178.B target: only fixture artifacts not already covered by
# `build-examples` (the two Rust `--id` images).
[group("examples")]
build-fixture-extras:
    #!/usr/bin/env bash
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    set -e
    source scripts/build/cargo.sh
    source scripts/build/fixture-matrix.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    cmake_frontends="$(nros_cmake_frontend_jobs)"
    if ! threadx_dir="$(nros_locate_source threadx)"; then
        nros_lane_skip_note threadx-linux "the ThreadX kernel is not provisioned (nros locate threadx)"; exit 0
    fi
    echo "Building ThreadX-Linux test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ] || [ "${NROS_INHERIT_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    # The logging smoke, pool-exhaustion and condvar-contract (issue 1906)
    # images are owned-entry workspace rows since phase-531 W7, built by the
    # `workspace-fixtures-build.sh threadx-linux rust` call in `build-examples`.
    # phase-531 W6 / issue 2433 — every threadx-linux C and C++ talker witness
    # is a one-package workspace now (the C one an owned entry sized by its
    # census, the C++ one a component; RFC-0122 D10), built by `build-examples`
    # through its `[[workspace_fixture]]` rows, so no C/C++ leaf pass is left.

# Build every ThreadX Linux sched task: core libraries, examples, and test fixtures.
[group("full-matrix")]
build-all: build-fixtures
    @echo "ThreadX Linux build-all completed."

# Run ThreadX Linux integration tests
[group("main")]
test verbose="": build-fixtures
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh threadx_linux test "just test threadx_linux"
    set -e
    source scripts/build/cargo.sh
    nros_locate_sdk_trees  # phase-484 W2 — the trees W3c stopped exporting
    if [ ! -d "$THREADX_DIR/common/inc" ]; then
        echo "ERROR: ThreadX not found at $THREADX_DIR. Run: just threadx-linux setup"
        exit 1
    fi
    if [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "ERROR: NetX Duo not found at $NETX_DIR. Run: just threadx-linux setup"
        exit 1
    fi
    # The `threadx_linux` target is gone (2026-08-21): its only test read three
    # `is_*_available()` booleans and printed them, asserting nothing, so the
    # lane reported PASS with no ThreadX, no NetX Duo and no router. The file's
    # own header already said the substance lived in `rtos_e2e` — this selects
    # those cells, which is what the lane was always meant to run.
    just _nextest-platform rtos_e2e '{{verbose}}' '' 'test(ThreadxLinux)'

# Run all ThreadX Linux tests including networked E2E
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    # The `threadx_linux` target is gone (2026-08-21) — its one test printed
    # three `is_*_available()` booleans and asserted nothing. The e2e run below
    # was always the substance of this lane.
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(ThreadxLinux)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${e2e_args[@]}"

# CI: check prerequisites + test

# Phase 121.6 — build the ThreadX-linux smoke test for
# nros-platform-threadx. Uses the in-tree ThreadX linux/gnu port +
# in-crate platform.c + timer.c. Excludes net.c (no NetX Duo).
[group("examples")]
build-c-port:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    threadx_dir="$(nros_locate_source threadx)" || { echo "ThreadX kernel not provisioned — nros setup --source threadx" >&2; exit 1; }
    cd tests/threadx-c-smoke
    # nros-cmake-prefix-exempt: pure-C ThreadX port smoke test, no Rust.
    cmake -B build -DTHREADX_DIR="$threadx_dir" >/dev/null
    cmake --build build

# Phase 121.6 — run the ThreadX-linux smoke test on the host: the REAL kernel
# over the port's platform.c (clock, alloc incl. the exhausted-pool NULL of
# issue 1717 and the realloc probe of 1719, mutex, timer).
#
# Issue 1720 — runs in the nightly per-platform sweep (nightly.yml `platform`,
# threadx_linux cell). The kernel is a submodule the gate tier does not check
# out, so its absence is a REPORTED skip (`nros_check_unverified`), never a
# silent pass, and a lane that provisions it sets NROS_CHECK_SKIP_STRICT=1.
[group("debug")]
test-c-port:
    #!/usr/bin/env bash
    set -euo pipefail
    source scripts/build/cargo.sh
    if ! nros_locate_source threadx >/dev/null; then
        source scripts/build/check-skip.sh
        nros_check_unverified threadx-c-port-smoke "the ThreadX kernel is not provisioned (nros locate threadx) — nros setup --source threadx" || exit 1
        exit 0
    fi
    "{{just_executable()}}" threadx_linux build-c-port
    # issue 1741 — an image is bounded by a deadline that ESCALATES.
    . scripts/lib/deadline.sh
    "${NROS_DEADLINE[@]}" 15 ./tests/threadx-c-smoke/build/nros_threadx_c_smoke

[group("full-matrix")]
ci: doctor test

# =============================================================================
# Run examples (native Linux binaries)
#
# The ThreadX Linux network driver uses /dev/net/tun to attach to persistent
# user-owned TAP interfaces created by `just setup-network`. No CAP_NET_RAW
# or other file capabilities are needed.
# =============================================================================

# Helper: build and run.
_run example:
    #!/usr/bin/env bash
    set -e
    # phase-336 — the ambient profile (a hosted ThreadX process has no timing
    # floor or codegen carve-out), with the run path derived from it.
    source "{{justfile_directory()}}/scripts/build/cargo.sh"
    # issue 0488 residue 2 — build into the SHARED group, not a per-leaf
    # `target-zenoh/`. `threadx-linux` is in `NROS_FIXTURE_SHARED_PLATFORMS`, so
    # the fixture lane already compiles this exact configuration once for the
    # whole platform; an authored dir here re-did all of it per leaf. The flag
    # and the run path come from ONE `nros_fixture_group` call, so the recipe
    # cannot look where it did not write (phase-340 item 7).
    source "{{justfile_directory()}}/scripts/build/fixtures-target-dir.sh"
    args=""
    leaf="examples/threadx-linux/rust/{{example}}"
    tdir_flag="$(nros_fixture_target_dir_flag threadx-linux "$args" "")"
    artifact_dir="$(nros_fixture_row_artifact_dir "$leaf" threadx-linux "$args" "")"
    cd "{{justfile_directory()}}/$leaf"
    # shellcheck disable=SC2086
    cargo build $(nros_cargo_profile_arg_string) $args $tdir_flag
    exec "$artifact_dir/$(nros_cargo_target_profile_dir)/threadx-linux-{{example}}"

# Run ThreadX Linux talker
talker: (_run "talker")

# Run ThreadX Linux listener
listener: (_run "listener")

# Start zenohd router for ThreadX Linux examples (allocator rust-pubsub port 9000, bridge-networked)
zenohd model="":
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    nros_router_exec tcp/0.0.0.0:9000 "{{model}}"

# Clean ThreadX Linux build artifacts
[group("main")]
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-linux-zenoh
    rm -rf packages/testing/nros-tests/bins/logging-smoke-threadx-linux/target
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/threadx-linux/rust/$example/target-zenoh"
        rm -rf "examples/threadx-linux/c/$example/build-zenoh"
        rm -rf "examples/threadx-linux/c/$example/build-cyclonedds"
        rm -rf "examples/threadx-linux/cpp/$example/build-zenoh"
        rm -rf "examples/threadx-linux/cpp/$example/build-cyclonedds"
    done
    echo "ThreadX Linux build artifacts cleaned"
