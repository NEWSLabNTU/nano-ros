set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX RISC-V 64 prerequisites: initialize submodules.
# Does NOT build example binaries — use `just threadx_riscv64 build-examples` for that.
# Phase 123.A.8 — submodule fetch delegates to tools/setup.sh.
[group("setup")]
setup:
    @"{{justfile_directory()}}/tools/setup.sh" --platform=threadx --skip-apt-check

# Diagnose ThreadX RISC-V 64 install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$THREADX_DIR/common/inc" ]; then
        echo "  [OK] ThreadX kernel: $THREADX_DIR"
    else
        echo "  [MISSING] ThreadX kernel at $THREADX_DIR (run: just threadx_riscv64 setup)"
        fail=1
    fi
    if [ -d "$NETX_DIR/common/inc" ]; then
        echo "  [OK] NetX Duo: $NETX_DIR"
    else
        echo "  [MISSING] NetX Duo at $NETX_DIR (run: just threadx_riscv64 setup)"
        fail=1
    fi
    if command -v riscv64-unknown-elf-gcc &>/dev/null; then
        echo "  [OK] riscv64-unknown-elf-gcc"
        # The Ubuntu `gcc-riscv64-unknown-elf` package is a bare-metal
        # compiler with NO C library — the C/C++ examples need libc
        # headers (stdint.h etc.) from picolibc, found via
        # `--specs=picolibc.specs` (the same way `riscv64-threadx.cmake`
        # locates the picolibc sysroot). Probe by preprocessing a TU
        # that pulls in stdint.h under those specs; a compiler-only
        # install fails with `stdint.h: No such file or directory`.
        if echo '#include <stdint.h>' | riscv64-unknown-elf-gcc --specs=picolibc.specs -E - &>/dev/null; then
            echo "  [OK] riscv64 C library headers (picolibc)"
        else
            echo "  [MISSING] riscv64 C library headers — install picolibc:"
            echo "             sudo apt install picolibc-riscv64-unknown-elf"
            echo "             (or: just workspace apt-packages). Needed only for the"
            echo "             C/C++ riscv64 examples; the Rust ones build without it."
            fail=1
        fi
    else
        echo "  [MISSING] riscv64-unknown-elf-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if command -v qemu-system-riscv64 &>/dev/null; then
        echo "  [OK] qemu-system-riscv64"
    else
        echo "  [MISSING] qemu-system-riscv64 (run: just workspace apt-packages)"
        fail=1
    fi
    exit $fail

# Phase 140 — `install` recipe deleted. ThreadX RISC-V 64 examples
# consume nano-ros via `add_subdirectory(<repo-root>)` (Phase 144.7).

# Build ThreadX RISC-V 64 examples (canonical core-link smoke).
[group("main")]
build: build-examples
    @echo "ThreadX RISC-V 64 examples built."

# Build ThreadX QEMU RISC-V 64 examples
[group("main")]
build-examples:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "Skipping ThreadX QEMU RISC-V examples (ThreadX/NetX not found). Run: just threadx-linux setup"
        exit 0
    fi
    if ! command -v riscv64-unknown-elf-gcc &>/dev/null; then
        echo "Skipping ThreadX QEMU RISC-V examples (riscv64-unknown-elf-gcc not found)"
        exit 0
    fi
    echo "Building ThreadX QEMU RISC-V 64 examples..."
    rv_config="$(pwd)/packages/boards/nros-board-threadx-qemu-riscv64/config"
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-riscv64-threadx/rust/$example && \
         THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config" \
           cargo build $cargo_profile_args --no-default-features --features rmw-zenoh \
             --target-dir target-zenoh)
    done
    echo "ThreadX QEMU RISC-V examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
[group("full-matrix")]
build-fixtures: build-examples build-fixture-extras
    @echo "ThreadX-RV64 test fixtures built."

# Internal Phase 178.B target: only fixture artifacts not already covered by
# `build-examples`. Mirrors the cmake -D set used by the test fixture builders
# in `nros_tests::fixtures::binaries::threadx_riscv64::build_cmake_example`.
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
    if ! command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
        echo "ThreadX-RV64 skip: riscv64-unknown-elf-gcc not found"; exit 0
    fi
    echo "Building ThreadX-RV64 test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    root="$(pwd)"
    toolchain="$root/cmake/toolchain/riscv64-threadx.cmake"
    threadx_dir="${THREADX_DIR:-$root/third-party/threadx/kernel}"
    netx_dir="${NETX_DIR:-$root/third-party/threadx/netxduo}"
    config_dir="$root/packages/boards/nros-board-threadx-qemu-riscv64/config"
    board_dir="$root/packages/boards/nros-board-threadx-qemu-riscv64/c"
    virtio_dir="$root/packages/drivers/virtio-net-netx"
    # The normal Rust role examples are produced by `build-examples`.
    # Only the standalone smoke fixture builds from a fixed path here.
    rust_dirs=()
    # Phase 88.15.d — nros-log severity-coverage smoke fixture.
    rust_dirs+=("packages/testing/nros-tests/bins/logging-smoke-threadx-riscv64")
    # threadx-netx-sys/build.rs needs the rv64-specific NetX port
    # headers, not the threadx-linux defaults exported by the top-level
    # justfile. Pass via env to each parallel cargo.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        for dir in "${rust_dirs[@]}"; do
            echo "  → $dir"
            ( cd "$dir" && \
              THREADX_CONFIG_DIR="$config_dir" NETX_CONFIG_DIR="$config_dir" \
              cargo build $cargo_profile_args --quiet )
        done
    else
        printf '%s\n' "${rust_dirs[@]}" | parallel --jobs "$cargo_frontends" --halt now,fail=1 --line-buffer \
            "echo '  → {}' && cd {} && \
             THREADX_CONFIG_DIR='$config_dir' NETX_CONFIG_DIR='$config_dir' \
             cargo build \$cargo_profile_args --quiet"
    fi
    # Phase 160.J — ThreadX RV64 C/C++ examples now build via cmake.
    # Corrosion handles the riscv64gc-unknown-none-elf cross under
    # the bundled toolchain file; the ThreadX + NetX Duo include
    # paths the staticlib needs flow in via `-D` cache vars matching
    # the Rust path's `.cargo/config.toml` env. Host nros-codegen is
    # still required for `nros_generate_interfaces()`.
    echo "  → host nros-codegen (cargo)"
    cargo build $cargo_profile_args --manifest-path packages/codegen/packages/Cargo.toml \
        -p nros-codegen-c --bin nros-codegen --quiet
    codegen="$root/packages/codegen/packages/target/$(nros_cargo_target_profile_dir)/nros-codegen"
    # Phase 118.C — collapsed-shape ThreadX-RV64 C / C++. The Rust
    # zenoh role binaries are produced by `build-examples`.
    # Phase 175.B adds CMake/Corrosion Cyclone cells for talker/listener
    # coverage because Cyclone is a C++ backend and cannot link from
    # cargo alone.

    build_threadx_cmake_rmw() {
        local dir="$1"
        local rmw="$2"
        local build_dir="$3"
        echo "  → $dir (-DNROS_RMW=$rmw, $build_dir/)"
        local sig
        sig="$(printf '%s\n' \
          "platform=threadx_riscv64" \
          "rmw=$rmw" \
          "src=$dir" \
          "toolchain=$toolchain" \
          "threadx_dir=$threadx_dir" \
          "netx_dir=$netx_dir" \
          "config_dir=$config_dir" \
          "codegen=$codegen" \
          "cyclonedds_prefix=${cyclonedds_prefix:-}")"
        THREADX_DIR="$threadx_dir" NETX_DIR="$netx_dir" \
        THREADX_CONFIG_DIR="$config_dir" NETX_CONFIG_DIR="$config_dir" \
          nros_cmake_fixture_build "$dir" "$dir/$build_dir" "$sig" \
            -DNROS_RMW="$rmw" \
            -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
            -DTHREADX_DIR="$threadx_dir" \
            -DNETX_DIR="$netx_dir" \
            -DTHREADX_CONFIG_DIR="$config_dir" \
            -DNETX_CONFIG_DIR="$config_dir" \
            -DCMAKE_BUILD_TYPE=Release \
            -D_NANO_ROS_CODEGEN_TOOL="$codegen" \
            -DIDLC_EXECUTABLE="$root/build/install/bin/idlc" \
            -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$root/scripts/cyclonedds/msg_to_cyclone_idl.py" > /dev/null
    }

    collapsed_cmake_dirs=()
    for lang in c cpp; do
        for ex in $(nros_fixture_roles); do
            dir="examples/qemu-riscv64-threadx/$lang/$ex"
            [ -d "$dir" ] && collapsed_cmake_dirs+=("$dir")
        done
    done
    if [ ${#collapsed_cmake_dirs[@]} -gt 0 ]; then
        export root toolchain threadx_dir netx_dir config_dir codegen
        export -f nros_cmake_fixture_build build_threadx_cmake_rmw
        if [ "${NROS_JOBSERVER:-}" = "1" ]; then
            for dir in "${collapsed_cmake_dirs[@]}"; do
                build_threadx_cmake_rmw "$dir" zenoh build-zenoh
            done
        else
            printf '%s\n' "${collapsed_cmake_dirs[@]}" | parallel --jobs "$cmake_frontends" --halt now,fail=1 --line-buffer \
                "build_threadx_cmake_rmw {} zenoh build-zenoh"
        fi
    fi

    cyclonedds_prefix="$root/build/cyclonedds-threadx-rv64-install"
    if [ -f "$cyclonedds_prefix/lib/libddsc.a" ] && [ -x "$root/build/install/bin/idlc" ]; then
        cyclone_dirs=(
            "examples/qemu-riscv64-threadx/c/talker"
            "examples/qemu-riscv64-threadx/c/listener"
            "examples/qemu-riscv64-threadx/cpp/talker"
            "examples/qemu-riscv64-threadx/cpp/listener"
            "examples/qemu-riscv64-threadx/rust/talker"
        )
        for dir in "${cyclone_dirs[@]}"; do
            CMAKE_PREFIX_PATH="$cyclonedds_prefix" \
              build_threadx_cmake_rmw "$dir" cyclonedds build-cyclonedds
        done
    else
        echo "  → skipping ThreadX-RV64 Cyclone fixtures (run: just cyclonedds setup && just cyclonedds threadx-cross-probe)"
    fi

# Build every ThreadX RISC-V 64 tier: core libraries, examples, and test fixtures.
[group("full-matrix")]
build-all: build-fixtures
    @echo "ThreadX RISC-V 64 build-all completed."

# Run ThreadX QEMU RISC-V 64 integration tests
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
    rv_config="$(pwd)/packages/boards/nros-board-threadx-qemu-riscv64/config"
    export THREADX_CONFIG_DIR="$rv_config"
    export NETX_CONFIG_DIR="$rv_config"
    just _nextest-platform threadx_riscv64_qemu '{{verbose}}'

# Run all ThreadX RISC-V tests including networked E2E
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    args=(-p nros-tests --test threadx_riscv64_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(ThreadxRiscv64)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${e2e_args[@]}"

# CI: check prerequisites + test
[group("full-matrix")]
ci: doctor test

# =============================================================================
# Run examples (build + launch QEMU RISC-V 64 virt with slirp networking)
#
# QEMU ignores SIGINT — each recipe runs QEMU in the background and
# traps INT/TERM to kill it explicitly (Ctrl-C works).
# peer_index: 0=talker/server (MAC :56), 1=listener/client (MAC :57)
# =============================================================================

[no-exit-message]
_run-qemu example binary peer_index:
    #!/usr/bin/env bash
    set -e
    rv_config="$(pwd)/packages/boards/nros-board-threadx-qemu-riscv64/config"
    cd "examples/qemu-riscv64-threadx/rust/{{example}}"
    THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config" cargo build --release --no-default-features --features rmw-zenoh --target-dir target-zenoh
    MAC="52:54:00:12:34:$(printf '%02x' $((0x56 + {{peer_index}})))"
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    qemu-system-riscv64 -M virt -m 256M -bios none -nographic \
        -global virtio-mmio.force-legacy=false \
        -kernel "target-zenoh/riscv64gc-unknown-none-elf/release/{{binary}}" \
        -netdev user,id=net0 \
        -device "virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac=$MAC" &
    PID=$!
    wait $PID

# Run ThreadX RISC-V talker on QEMU
talker: (_run-qemu "talker" "qemu-riscv64-threadx-talker" "0")

# Run ThreadX RISC-V listener on QEMU
listener: (_run-qemu "listener" "qemu-riscv64-threadx-listener" "1")

# Run ThreadX RISC-V service server on QEMU
service-server: (_run-qemu "service-server" "qemu-riscv64-threadx-service-server" "0")

# Run ThreadX RISC-V service client on QEMU
service-client: (_run-qemu "service-client" "qemu-riscv64-threadx-service-client" "1")

# Run ThreadX RISC-V action server on QEMU
action-server: (_run-qemu "action-server" "qemu-riscv64-threadx-action-server" "0")

# Run ThreadX RISC-V action client on QEMU
action-client: (_run-qemu "action-client" "qemu-riscv64-threadx-action-client" "1")

# Start zenohd router for ThreadX RISC-V examples (port 7453)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7453 --no-multicast-scouting

# Clean ThreadX RISC-V build artifacts
[group("main")]
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-riscv64-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-riscv64-threadx/rust/$example/target" \
               "examples/qemu-riscv64-threadx/rust/$example/target-zenoh"
        rm -rf "examples/qemu-riscv64-threadx/c/$example/build-zenoh" \
               "examples/qemu-riscv64-threadx/cpp/$example/build-zenoh"
    done
    echo "ThreadX RISC-V build artifacts cleaned"
