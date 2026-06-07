set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX RISC-V 64 prerequisites. Phase 197.4 — `nros setup <board>`
# provisions the threadx + netxduo sources + riscv-none-elf-gcc + qemu from the
# SDK index; the host-env helper handles rustup (riscv64 build-std, no cross
# target). Does NOT build example binaries — use `build-examples`.
[group("setup")]
setup:
    nros setup qemu-riscv64-threadx
    @"{{justfile_directory()}}/tools/host-env.sh" threadx

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
    # Board config dirs are platform env (same for every fixture) — export them
    # for the shared builder; per-fixture options come from the SSOT manifest
    # (examples/fixtures.toml). Phase 181.4.g.
    rv_config="$(pwd)/packages/boards/nros-board-threadx-qemu-riscv64/config"
    export THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config"
    bash scripts/build/fixtures-build.sh threadx-riscv64 rust
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
    # Phase 88.15.d — nros-log severity-coverage smoke fixture. The RV64
    # threadx-netx-sys build needs this board's NetX port headers rather than
    # the threadx-linux defaults exported by the top-level justfile.
    THREADX_CONFIG_DIR="$config_dir" NETX_CONFIG_DIR="$config_dir" \
        bash scripts/build/fixtures-build.sh threadx-riscv64 rust --id threadx-riscv64-logging-smoke
    # Phase 160.J — ThreadX RV64 C/C++ examples now build via cmake.
    # Corrosion handles the riscv64gc-unknown-none-elf cross under
    # the bundled toolchain file; the ThreadX + NetX Duo include
    # paths the staticlib needs flow in via `-D` cache vars matching
    # the Rust path's `.cargo/config.toml` env. Host nros-codegen is
    # still required for `nros_generate_interfaces()`.
    echo "  → host nros-codegen (cargo)"
    nros_cargo_ensure_codegen_c
    codegen="$(nros_cargo_codegen_c_bin)"
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
        # Phase 177.38 — Cyclone fixtures bake domain 62 (env for the
        # Rust/Corrosion build + -D for the C/C++ glue); fold it into the cache
        # sig so a domain change forces a rebuild. Empty for non-Cyclone (keeps
        # the config.toml default).
        local _dom=""
        [ "$rmw" = "cyclonedds" ] && _dom="62"
        local sig
        sig="$(printf '%s\n' \
          "platform=threadx_riscv64" \
          "rmw=$rmw" \
          "domain=${_dom}" \
          "src=$dir" \
          "toolchain=$toolchain" \
          "threadx_dir=$threadx_dir" \
          "netx_dir=$netx_dir" \
          "config_dir=$config_dir" \
          "codegen=$codegen" \
          "cyclonedds_prefix=${cyclonedds_prefix:-}")"
        # Export the domain (not an inline VAR=val prefix — a prefix produced by
        # ${_dom:+…} expansion is parsed as a command, not an assignment). The
        # Rust/Corrosion build reads it via option_env!; the -D below covers the
        # C/C++ glue. Only set for Cyclone (_dom non-empty).
        [ -n "$_dom" ] && export NROS_DOMAIN_ID="$_dom"
        THREADX_DIR="$threadx_dir" NETX_DIR="$netx_dir" \
        THREADX_CONFIG_DIR="$config_dir" NETX_CONFIG_DIR="$config_dir" \
          nros_cmake_fixture_build "$dir" "$dir/$build_dir" "$sig" \
            -DNROS_RMW="$rmw" \
            ${_dom:+-DNROS_DOMAIN_ID=$_dom} \
            -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
            -DTHREADX_DIR="$threadx_dir" \
            -DNETX_DIR="$netx_dir" \
            -DTHREADX_CONFIG_DIR="$config_dir" \
            -DNETX_CONFIG_DIR="$config_dir" \
            -DCMAKE_BUILD_TYPE=Release \
            -D_NANO_ROS_CODEGEN_TOOL="$codegen" \
            -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL="$root/scripts/cyclonedds/msg_to_cyclone_idl.py" > /dev/null
    }

    # Phase 181.5.e — collapsed-shape ThreadX-RV64 C / C++ from the SSOT
    # manifest. The Rust zenoh role binaries are produced by `build-examples`.
    # The example CMakeLists read the NetX / ThreadX include paths from env at
    # configure time (matching the Rust path's .cargo/config.toml), so export
    # them; the rest of the -D set goes through NROS_CMAKE_EXTRA_DEFS. The
    # IDLC / msg_to_idl defs are harmless for the zenoh pass.
    export THREADX_DIR="$threadx_dir" NETX_DIR="$netx_dir" \
           THREADX_CONFIG_DIR="$config_dir" NETX_CONFIG_DIR="$config_dir"
    base_defs="-DCMAKE_TOOLCHAIN_FILE=$toolchain -DTHREADX_DIR=$threadx_dir -DNETX_DIR=$netx_dir -DTHREADX_CONFIG_DIR=$config_dir -DNETX_CONFIG_DIR=$config_dir -DCMAKE_BUILD_TYPE=Release -D_NANO_ROS_CODEGEN_TOOL=$codegen -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=$root/scripts/cyclonedds/msg_to_cyclone_idl.py"
    export NROS_CMAKE_EXTRA_DEFS="$base_defs"
    bash scripts/build/fixtures-build.sh threadx-riscv64 c zenoh
    bash scripts/build/fixtures-build.sh threadx-riscv64 cpp zenoh

    cyclonedds_prefix="$root/build/cyclonedds-threadx-rv64-install"
    # Phase 203 decision — build the ThreadX-RV64 Cyclone fixtures BY DEFAULT so a
    # clean `build-all` compiles every fixture (the runtime works e2e since Phase
    # 177.26; Phase 186 self-provisions Cyclone from source, no separate probe).
    # Opt out with NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0 (e.g. a fast local loop).
    if [ "${NROS_THREADX_RV64_CYCLONEDDS_FIXTURES:-1}" = "0" ]; then
        echo "  → skipping ThreadX-RV64 Cyclone fixtures (NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0 opt-out)"
    elif command -v riscv64-unknown-elf-gcc >/dev/null 2>&1; then
        # Phase 186 — self-provision Cyclone from source via CMake
        # (nros_provide_cyclonedds() + the ThreadX flags staged in
        # cmake/platform/nano-ros-threadx.cmake); no prebuilt install, no
        # `just cyclonedds threadx-cross-probe`. idlc resolves host-side (186.3).
        # Phase 177.38 — ThreadX-RV64 Cyclone role-set on domain 62 (talker+listener
        # share). C/C++ get it via cmake -DNROS_DOMAIN_ID; the rust/talker cell via
        # the helper's env + -D.
        export NROS_CMAKE_EXTRA_DEFS="$base_defs -DNROS_DOMAIN_ID=62"
        bash scripts/build/fixtures-build.sh threadx-riscv64 c cyclonedds
        bash scripts/build/fixtures-build.sh threadx-riscv64 cpp cyclonedds
        if [ -d examples/qemu-riscv64-threadx/rust/talker ]; then
            build_threadx_cmake_rmw examples/qemu-riscv64-threadx/rust/talker cyclonedds build-cyclonedds
        fi
    else
        echo "  → skipping ThreadX-RV64 Cyclone fixtures (riscv64-unknown-elf-gcc absent — cannot self-provision Cyclone)"
    fi
    unset NROS_CMAKE_EXTRA_DEFS

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
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --test threadx_riscv64_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(ThreadxRiscv64)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${e2e_args[@]}"

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
    zenohd --listen tcp/127.0.0.1:7453 --no-multicast-scouting

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
