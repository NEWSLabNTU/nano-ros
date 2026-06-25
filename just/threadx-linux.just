set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX Linux prerequisites. Phase 197.4 — `nros setup <board>`
# provisions the threadx + netxduo sources (+ tools) from the SDK index; the
# host-env helper handles rustup. Host build, so no cross target.
# Does NOT build example binaries — use `just threadx_linux build-examples` for that.
[group("setup")]
setup:
    nros setup threadx-linux
    @"{{justfile_directory()}}/tools/host-env.sh" threadx

# Diagnose ThreadX Linux install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
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
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "Skipping ThreadX Linux examples (ThreadX/NetX not found). Run: just threadx-linux setup"
        exit 0
    fi
    echo "Building ThreadX Linux examples..."
    # Build options from the SSOT manifest (examples/fixtures.toml). Phase 181.4.f.
    bash scripts/build/fixtures-build.sh threadx-linux rust
    bash scripts/build/workspace-fixtures-build.sh threadx-linux rust
    # phase-263 C2a — the embedded C workspace LAUNCH entry (issue 0097), consumed
    # by tests/c_threadx_entry_e2e.rs. Builds into its own build-subdir (one board
    # per CMake configure), so it never clashes with the posix workspace fixtures.
    bash scripts/build/workspace-fixtures-build.sh threadx-linux c
    # phase-263 C2c — the embedded C++ workspace LAUNCH entry, consumed by
    # tests/cpp_threadx_entry_e2e.rs.
    bash scripts/build/workspace-fixtures-build.sh threadx-linux cpp
    echo "ThreadX Linux examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
[group("full-matrix")]
build-fixtures: build-examples build-fixture-extras
    @echo "ThreadX-Linux test fixtures built."

# Internal Phase 178.B target: only fixture artifacts not already covered by
# `build-examples`. Mirrors the cmake -D set used by the test fixture builders
# in `nros_tests::fixtures::binaries::threadx_linux::build_cmake_example`.
[group("examples")]
build-fixture-extras:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    source scripts/build/fixture-matrix.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    cmake_frontends="$(nros_cmake_frontend_jobs)"
    if [ -z "${THREADX_DIR:-}" ] && [ ! -d third-party/threadx/kernel ]; then
        echo "ThreadX-Linux skip: THREADX_DIR unset and third-party/threadx/kernel absent"; exit 0
    fi
    echo "Building ThreadX-Linux test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    root="$(pwd)"
    threadx_dir="${THREADX_DIR:-$root/third-party/threadx/kernel}"
    nsos_netx_dir="${NSOS_NETX_DIR:-$root/packages/drivers/nsos-netx}"
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
    export NROS_CMAKE_EXTRA_DEFS="-DNANO_ROS_PLATFORM=threadx_linux -DTHREADX_DIR=$threadx_dir -DNSOS_NETX_DIR=$nsos_netx_dir -DTHREADX_CONFIG_DIR=$config_dir -DTHREADX_APP_DEFINE=$app_define -DCMAKE_BUILD_TYPE=Release -DCMAKE_DISABLE_FIND_PACKAGE_CycloneDDS=ON -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=$msg_to_idl -D_NANO_ROS_CODEGEN_TOOL=$codegen"
    bash scripts/build/fixtures-build.sh threadx-linux c zenoh
    bash scripts/build/fixtures-build.sh threadx-linux cpp zenoh
    if command -v idlc >/dev/null 2>&1 || [ -x build/install/bin/idlc ]; then
        # Phase 177.38 — bake this platform's Cyclone role-set onto domain 61
        # (C/C++ → cmake -DNROS_DOMAIN_ID via NanoRosConfig.cmake) so its host
        # RTPS ports stay disjoint from other host Cyclone fixtures. Only the
        # cyclonedds pass gets it; zenoh keeps the config.toml default. The paired
        # native listener spawns at domain 61 in native_api.rs's
        # test_threadx_linux_cyclonedds_talker_to_native_listener.
        NROS_CMAKE_EXTRA_DEFS="$NROS_CMAKE_EXTRA_DEFS -DNROS_DOMAIN_ID=61" \
            bash scripts/build/fixtures-build.sh threadx-linux c cyclonedds
        NROS_CMAKE_EXTRA_DEFS="$NROS_CMAKE_EXTRA_DEFS -DNROS_DOMAIN_ID=61" \
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
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ]; then
        echo "ERROR: ThreadX not found at $THREADX_DIR. Run: just threadx-linux setup"
        exit 1
    fi
    if [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "ERROR: NetX Duo not found at $NETX_DIR. Run: just threadx-linux setup"
        exit 1
    fi
    just _nextest-platform threadx_linux '{{verbose}}'

# Run all ThreadX Linux tests including networked E2E
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test threadx_linux --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
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
    cd "examples/threadx-linux/rust/{{example}}"
    cargo build --release --no-default-features --features rmw-zenoh --target-dir target-zenoh
    exec target-zenoh/release/threadx-linux-{{example}}

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

# Start zenohd router for ThreadX Linux examples (port 7455, bridge-networked)
zenohd:
    zenohd --listen tcp/0.0.0.0:7455 --no-multicast-scouting

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
