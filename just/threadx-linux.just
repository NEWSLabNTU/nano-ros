set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX Linux prerequisites: initialize submodules.
# Does NOT build example binaries — use `just threadx_linux build-examples` for that.
# Phase 123.A.8 — submodule fetch delegates to tools/setup.sh.
setup:
    @"{{justfile_directory()}}/tools/setup.sh" --platform=threadx --skip-apt-check

# Diagnose ThreadX Linux install status (read-only).
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
build: build-examples
    @echo "ThreadX Linux examples built."

# Build ThreadX Linux examples
build-examples:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "Skipping ThreadX Linux examples (ThreadX/NetX not found). Run: just threadx-linux setup"
        exit 0
    fi
    echo "Building ThreadX Linux examples..."
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/threadx-linux/rust/zenoh/$example && cargo build --release)
    done
    echo "ThreadX Linux examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
# Mirrors the cmake -D set used by the test fixture builders in
# `nros_tests::fixtures::binaries::threadx_linux::build_cmake_example`.
build-fixtures:
    #!/usr/bin/env bash
    set -e
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
    # Phase 160 follow-up — parallel cargo builds.
    # Phase 118 collapsed the per-RMW `rust/<rmw>/<ex>` dirs onto a
    # single `rust/<ex>` (RMW via Cargo features); the collapsed loop
    # below drives those, so there are no fixed per-RMW Rust paths to
    # build up front here.

    echo "  → host nros-codegen (cargo)"
    cargo build --release --manifest-path packages/codegen/packages/Cargo.toml \
        -p nros-codegen-c --bin nros-codegen --quiet
    codegen="$(pwd)/packages/codegen/packages/target/release/nros-codegen"
    cmake_dirs=()
    for lang in c cpp; do
        for ex in talker listener service-server service-client action-server action-client; do
            dir="examples/threadx-linux/$lang/zenoh/$ex"
            [ -d "$dir" ] && cmake_dirs+=("$dir")
        done
    done
    [ ${#cmake_dirs[@]} -gt 0 ] && printf '%s\n' "${cmake_dirs[@]}" | parallel --jobs ${NROS_BUILD_JOBS:-4} --halt now,fail=1 --line-buffer \
        "echo '  → {} (cmake)' && rm -rf {}/build && \
         cmake -S {} -B {}/build \
           -DNANO_ROS_PLATFORM=threadx_linux \
           -DTHREADX_DIR='$threadx_dir' \
           -DNSOS_NETX_DIR='$nsos_netx_dir' \
           -DTHREADX_CONFIG_DIR='$config_dir' \
           -DTHREADX_APP_DEFINE='$app_define' \
           -DCMAKE_BUILD_TYPE=Release \
           -D_NANO_ROS_CODEGEN_TOOL='$codegen' > /dev/null && \
         cmake --build {}/build > /dev/null"

    # Phase 118.B.7 — collapsed-shape threadx-linux Rust + C / C++.
    collapsed_rust_dirs=()
    for case in talker listener service-server service-client action-server action-client; do
        if [ ! -d examples/threadx-linux/rust/${case} ]; then
            continue
        fi
        # Phase 169 retired dust-dds; Cyclone (its replacement) has no
        # pure-cargo build, so the migrated rmw-cyclonedds feature stays
        # defined but unbuilt — deferred to phase-175. zenoh-only here.
        rmws="zenoh"
        for rmw in $rmws; do
            echo "  → threadx-linux/rust/${case} (--features rmw-${rmw}, target-${rmw}/)"
            ( cd examples/threadx-linux/rust/${case} && \
              cargo build --release --no-default-features --features rmw-${rmw} \
                  --target-dir target-${rmw} --quiet ) &
        done
    done
    wait

    collapsed_cmake_dirs=()
    for lang in c cpp; do
        for ex in talker listener service-server service-client action-server action-client; do
            dir="examples/threadx-linux/$lang/$ex"
            [ -d "$dir" ] && collapsed_cmake_dirs+=("$dir")
        done
    done
    if [ ${#collapsed_cmake_dirs[@]} -gt 0 ]; then
        cmake_rmws="zenoh"
        if [ -f build/install/lib/libddsc.so ] && [ -x build/install/bin/idlc ]; then
            cmake_rmws="$cmake_rmws cyclonedds"
        else
            echo "  → threadx-linux c/cpp cyclonedds skipped (run: just cyclonedds setup)"
        fi
        printf '%s\n' "${collapsed_cmake_dirs[@]}" | parallel --jobs ${NROS_BUILD_JOBS:-4} --halt now,fail=1 --line-buffer \
            "for rmw in $cmake_rmws; do \
                 echo '  → {} (-DNROS_RMW='\$rmw', build-'\$rmw'/)' && rm -rf {}/build-\$rmw && \
                 cmake -S {} -B {}/build-\$rmw \
                   -DNROS_RMW=\$rmw \
                   -DNANO_ROS_PLATFORM=threadx_linux \
                   -DTHREADX_DIR='$threadx_dir' \
                   -DNSOS_NETX_DIR='$nsos_netx_dir' \
                   -DTHREADX_CONFIG_DIR='$config_dir' \
                   -DTHREADX_APP_DEFINE='$app_define' \
                   -DCMAKE_BUILD_TYPE=Release \
                   -D_NANO_ROS_CODEGEN_TOOL='$codegen' > /dev/null && \
                 cmake --build {}/build-\$rmw > /dev/null; \
             done"
    fi

# Build every ThreadX Linux tier: core libraries, examples, and test fixtures.
build-all: build build-examples build-fixtures
    @echo "ThreadX Linux build-all completed."

# Run ThreadX Linux integration tests
test verbose="": build-examples
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
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test threadx_linux --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(ThreadxLinux)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${e2e_args[@]}"

# CI: check prerequisites + test

# Phase 121.6 — build the ThreadX-linux smoke test for
# nros-platform-threadx. Uses the in-tree ThreadX linux/gnu port +
# in-crate platform.c + timer.c. Excludes net.c (no NetX Duo).
build-c-port:
    #!/usr/bin/env bash
    set -e
    cd tests/threadx-c-smoke
    cmake -B build >/dev/null
    cmake --build build

# Phase 121.6 — run the ThreadX-linux smoke test on the host.
test-c-port: build-c-port
    #!/usr/bin/env bash
    set -e
    timeout 15 ./tests/threadx-c-smoke/build/nros_threadx_c_smoke

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
    cd "examples/threadx-linux/rust/zenoh/{{example}}"
    cargo build --release
    exec target/release/threadx-linux-{{example}}

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
    build/zenohd/zenohd --listen tcp/0.0.0.0:7455 --no-multicast-scouting

# Clean ThreadX Linux build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-linux-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/threadx-linux/rust/zenoh/$example/target"
    done
    echo "ThreadX Linux build artifacts cleaned"
