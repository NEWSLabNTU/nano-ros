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

# Phase 123.A.1.x.3 — install the standalone ThreadX platform C-port
# (`libnros_platform_threadx.a`) into build/install/lib/. Builds
# against the in-tree ThreadX-Kernel headers via the linux/gnu port
# (host-buildable; embedded targets re-cross-build via their own
# scaffold copy). Net surface (`net.c`) is excluded — needs NetX Duo
# headers; apps that want it compile net.c into their binary via the
# board crate's build.rs.
install-platform:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "${THREADX_DIR:-third-party/threadx/kernel}/common/inc" ]; then
        echo "ThreadX SDK not found — skipping standalone platform C-port"
        exit 0
    fi
    PREFIX="$(pwd)/build/install"
    SCAFFOLD="$(pwd)/tools/install-platform/threadx"
    echo "=== Building standalone libnros_platform_threadx.a ==="
    cmake -S "$SCAFFOLD" -B build/cmake-platform-threadx \
        -DCMAKE_BUILD_TYPE=Release >/dev/null
    cmake --build build/cmake-platform-threadx
    cmake --install build/cmake-platform-threadx --prefix "$PREFIX"

# Install ThreadX Linux libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "${THREADX_DIR:-third-party/threadx/kernel}/common/inc" ]; then
        echo "ThreadX SDK not found — skipping ThreadX Linux libraries"
        exit 0
    fi
    PREFIX="$(pwd)/build/install"
    echo "=== Building threadx_linux RMW=zenoh ==="
    cmake -S . -B "build/cmake-threadx-linux-zenoh" \
        -DNANO_ROS_RMW="zenoh" \
        -DNANO_ROS_PLATFORM="threadx_linux" \
        -DNANO_ROS_BUILD_CODEGEN=OFF \
        -DCMAKE_BUILD_TYPE=Release
    just _cmake-cargo-stale-guard "build/cmake-threadx-linux-zenoh"
    cmake --build "build/cmake-threadx-linux-zenoh"
    cmake --install "build/cmake-threadx-linux-zenoh" --prefix "$PREFIX"

# Build ThreadX Linux platform/RMW libraries.
build: install-platform install
    @echo "ThreadX Linux core libraries built."

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
    root="$(pwd)"
    cmake_pfx="$root/build/install"
    threadx_dir="${THREADX_DIR:-$root/third-party/threadx/kernel}"
    nsos_netx_dir="${NSOS_NETX_DIR:-$root/packages/drivers/nsos-netx}"
    config_dir="${THREADX_CONFIG_DIR:-$root/packages/boards/nros-board-threadx-linux/config}"
    app_define="$root/packages/boards/nros-board-threadx-linux/c/app_define.c"
    rust_examples=(talker listener service-server service-client action-server action-client)
    for ex in "${rust_examples[@]}"; do
        echo "  → threadx-linux/rust/$ex"
        ( cd "examples/threadx-linux/rust/zenoh/$ex" && cargo build --release --quiet )
    done
    # Phase 97.4.threadx-linux — DDS variants alongside zenoh.
    dds_examples=(talker listener)
    for ex in "${dds_examples[@]}"; do
        echo "  → threadx-linux/rust/dds/$ex"
        ( cd "examples/threadx-linux/rust/dds/$ex" && cargo build --release --quiet )
    done
    cmake_examples=(talker listener service-server service-client action-server action-client)
    for lang in c cpp; do
        for ex in "${cmake_examples[@]}"; do
            dir="examples/threadx-linux/$lang/zenoh/$ex"
            [ ! -d "$dir" ] && continue
            echo "  → threadx-linux/$lang/$ex (cmake)"
            cmake -S "$dir" -B "$dir/build" \
                -DCMAKE_PREFIX_PATH="$cmake_pfx" \
                -DNANO_ROS_PLATFORM=threadx_linux \
                -DTHREADX_DIR="$threadx_dir" \
                -DNSOS_NETX_DIR="$nsos_netx_dir" \
                -DTHREADX_CONFIG_DIR="$config_dir" \
                -DTHREADX_APP_DEFINE="$app_define" \
                -DCMAKE_BUILD_TYPE=Release > /dev/null
            cmake --build "$dir/build" --parallel > /dev/null
        done
    done

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
