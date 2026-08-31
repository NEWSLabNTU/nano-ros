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
    # issue 0650 — first step of the lane: clear last run's skip notes, so a
    # lane that is complete this time is not reported SKIPPED from history.
    # shellcheck source=scripts/build/lane-skip.sh
    source scripts/build/lane-skip.sh
    nros_lane_skip_reset threadx-linux
    source scripts/build/cargo.sh
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
# `build-examples`. Mirrors the cmake -D set used by the test fixture builders
# in `nros_tests::fixtures::binaries::threadx_linux::build_cmake_example`.
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
    if [ -z "${THREADX_DIR:-}" ] && [ ! -d third-party/threadx/kernel ]; then
        nros_lane_skip_note threadx-linux "THREADX_DIR unset and third-party/threadx/kernel absent"; exit 0
    fi
    echo "Building ThreadX-Linux test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ] || [ "${NROS_INHERIT_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    root="$(pwd)"
    threadx_dir="${THREADX_DIR:-$root/third-party/threadx/kernel}"
    nsos_netx_dir="${NSOS_NETX_DIR:-$root/packages/drivers/net/nsos-netx}"
    config_dir="${THREADX_CONFIG_DIR:-$root/packages/boards/nros-board-threadx-linux/config}"
    app_define="$root/packages/boards/nros-board-threadx-linux/c/app_define.c"
    # Phase 177.27 — the cyclonedds RMW needs two flags native.just /
    # cyclonedds.just already pass; threadx-linux omitted both. Harmless for
    # the zenoh pass (it ignores them):
    #   1. msg_to_cyclone_idl.py — the .msg→IDL converter for
    #      nros_rmw_cyclonedds_generate_from_msg (find_program can't locate
    #      it in the source tree).
    #   2. CMAKE_PREFIX_PATH=build/install — so find_package(CycloneDDS)
    #      resolves the host ddsc/ddsrt libs; without it the fixtures fail to
    #      link (undefined dds_qset_*/ddsrt_*/dds_stream_* references).
    msg_to_idl="$root/scripts/cyclonedds/msg_to_cyclone_idl.py"
    cyclone_prefix="$root/build/install"
    echo "  → host nros-codegen (cargo)"
    nros_cargo_ensure_codegen_c
    codegen="$(nros_cargo_codegen_c_bin)"
    bash scripts/build/fixtures-build.sh threadx-linux rust --id threadx-linux-logging-smoke
    # Phase 181.5.d — collapsed-shape threadx-linux C / C++ from the SSOT
    # manifest. The Rust zenoh role binaries are produced by `build-examples`.
    # The full -D set (incl. the cyclone prefix / msg_to_idl, harmless for the
    # zenoh pass per the note above) is injected via NROS_CMAKE_EXTRA_DEFS and
    # reused for the gated cyclone pass; per-cell RMW + build-<rmw>/ subdir come
    # from the manifest.
    # Phase 186.6.3 — self-provision Cyclone from source (static host ddsc via
    # nano-ros-threadx.cmake's threadx-linux branch); DISABLE_FIND_PACKAGE forces
    # the pinned source over any system /opt/ros Cyclone. No build/install /
    # `just cyclonedds setup` needed; idlc resolves host-side (186.3).
    # issue 0805 — one Corrosion cargo dir per (platform, resolved feature
    # inputs) instead of one per leaf. cmake keys the subdirectory itself;
    # `nros_build_dir` verified IN SCOPE here first (a missing helper expands
    # the flag EMPTY and silently keeps per-leaf dirs — defect 3 on native).
    export NROS_CMAKE_EXTRA_DEFS="-DNROS_SHARED_CARGO_ROOT=$(nros_build_dir "$NROS_KIND_CORROSION_CARGO" threadx-linux) -DNANO_ROS_PLATFORM=threadx_linux -DTHREADX_DIR=$threadx_dir -DNSOS_NETX_DIR=$nsos_netx_dir -DTHREADX_CONFIG_DIR=$config_dir -DTHREADX_APP_DEFINE=$app_define -DCMAKE_BUILD_TYPE=Release -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=$msg_to_idl -D_NANO_ROS_CODEGEN_TOOL=$codegen"
    bash scripts/build/fixtures-build.sh threadx-linux c zenoh
    bash scripts/build/fixtures-build.sh threadx-linux cpp zenoh
    if command -v idlc >/dev/null 2>&1 || [ -x build/install/bin/idlc ]; then
        # phase-295 W4 (ex Phase 177.38) — bake each Cyclone lang pass onto its
        # ALLOCATOR domain (nros_tests::alloc::domain_of(ThreadxLinux, lang,
        # Pubsub): c=107, cpp=108; C/C++ → cmake -DNROS_DOMAIN_ID via
        # NanoRosConfig.cmake) so its host RTPS ports stay disjoint from every
        # other baked Cyclone fixture. Only the cyclonedds pass gets it; zenoh
        # keeps the config.toml default. The paired native listener spawns at
        # domain 107 in native_api.rs's
        # test_threadx_linux_cyclonedds_talker_to_native_listener.
        NROS_CMAKE_EXTRA_DEFS="$NROS_CMAKE_EXTRA_DEFS -DNROS_DOMAIN_ID=107" \
            bash scripts/build/fixtures-build.sh threadx-linux c cyclonedds
        NROS_CMAKE_EXTRA_DEFS="$NROS_CMAKE_EXTRA_DEFS -DNROS_DOMAIN_ID=108" \
            bash scripts/build/fixtures-build.sh threadx-linux cpp cyclonedds
    else
        echo "  → threadx-linux c/cpp cyclonedds skipped (idlc not found on PATH — install a CycloneDDS/ROS 2 idlc, or set IDLC_EXECUTABLE)"
    fi
    unset NROS_CMAKE_EXTRA_DEFS

# Build every ThreadX Linux tier: core libraries, examples, and test fixtures.
[group("full-matrix")]
build-all: build-fixtures
    @echo "ThreadX Linux build-all completed."

# Run ThreadX Linux integration tests
[group("main")]
test verbose="": build-fixtures
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh threadx_linux test "just test threadx_linux"
    set -e
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
        e2e_args+=(--success-output never --failure-output never)
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
    cd tests/threadx-c-smoke
    # nros-cmake-prefix-exempt: pure-C ThreadX port smoke test, no Rust.
    cmake -B build >/dev/null
    cmake --build build

# Phase 121.6 — run the ThreadX-linux smoke test on the host.
[group("debug")]
test-c-port: build-c-port
    #!/usr/bin/env bash
    set -e
    timeout 15 ./tests/threadx-c-smoke/build/nros_threadx_c_smoke

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
    args="--no-default-features --features rmw-zenoh"
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

# Run ThreadX Linux service server
service-server: (_run "service-server")

# Run ThreadX Linux service client
service-client: (_run "service-client")

# Run ThreadX Linux action server
action-server: (_run "action-server")

# Run ThreadX Linux action client
action-client: (_run "action-client")

# Start zenohd router for ThreadX Linux examples (allocator rust-pubsub port 9000, bridge-networked)
zenohd:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    nros_router_exec tcp/0.0.0.0:9000

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
