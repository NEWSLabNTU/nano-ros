set working-directory := '..'

# Phase 143 — patched qemu-system-arm built by `just qemu setup-qemu`.
QEMU_BIN := absolute_path("build/qemu") / "bin/qemu-system-arm"

default:
    @just --list --justfile {{source_file()}}

# Install FreeRTOS prerequisites. Phase 197.4 — `nros setup <board>` provisions
# the toolchain + sources (arm-none-eabi-gcc, qemu, FreeRTOS-Kernel, lwIP,
# zenoh-pico, …) from the SDK index; the host-env helper adds the cross Rust
# target. `source setup.bash` afterwards to put the provisioned tools on PATH.
[group("setup")]
setup:
    nros setup qemu-arm-freertos
    @"{{justfile_directory()}}/tools/host-env.sh" freertos

# Diagnose FreeRTOS install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$FREERTOS_DIR/include" ]; then
        echo "  [OK] FreeRTOS kernel: $FREERTOS_DIR"
    else
        echo "  [MISSING] FreeRTOS kernel at $FREERTOS_DIR (run: just freertos setup)"
        fail=1
    fi
    if [ -d "$LWIP_DIR/src/include" ]; then
        echo "  [OK] lwIP: $LWIP_DIR"
    else
        echo "  [MISSING] lwIP at $LWIP_DIR (run: just freertos setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    exit $fail

# Phase 140 — `install-platform` + `install` recipes deleted along with
# the `build/install/` layout. FreeRTOS examples consume nano-ros via
# `add_subdirectory(<repo-root>)` (Phase 144.5) or via the
# `integrations/nuttx/` etc. shells (Phase 139). The `build` recipe
# below now drives only the example tree.

# Build FreeRTOS examples (the canonical "did the core link clean?" smoke).
[group("main")]
build: build-examples
    @echo "FreeRTOS examples built."

# Build FreeRTOS QEMU MPS2-AN385 Rust examples
[group("main")]
build-examples:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    # Phase 214.J.2 — guard `ws sync` against stale `generated/` after trait
    # surface drift (see scripts/build/codegen-stamp.sh).
    source scripts/build/codegen-stamp.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    nros_cli="$(nros_cli_bin)"
    # Phase 214.I.2 — fail-loud prereq guard: the loop below invokes
    # `nros ws sync`, which the shipped 0.3.7 release predates. Probe
    # once, emit `[PREREQ]` + exit 0 if absent rather than cascading
    # into clap "unrecognized subcommand 'ws'" stack traces per example.
    nros_require_ws_sync "$nros_cli"
    echo "Building FreeRTOS QEMU MPS2-AN385 examples..."
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just setup-freertos"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just setup-freertos"
        exit 1
    fi
    # Codegen prep (per-platform); build options come from the SSOT manifest
    # (examples/fixtures.toml) via the shared builder. Phase 181.4.d.
    # Component pkgs AND their sibling Entry pkgs — the Entry pkg is the workspace
    # root carrying the `[patch.crates-io]` block, so it needs its own `generated/`
    # msg crates for the Phase 212.O.1 boot+connected gates to build the firmware.
    for example in \
        talker listener service-server service-client action-server action-client \
        talker-entry listener-entry service-server-entry service-client-entry \
        action-server-entry action-client-entry; do
        dir="examples/qemu-arm-freertos/rust/$example"
        NROS_REPO_DIR="$PWD" nros_codegen_stamp_check_or_wipe "$dir"
        NROS_REPO_DIR="$PWD" "$nros_cli" sync "$dir" >/dev/null
        NROS_REPO_DIR="$PWD" nros_codegen_stamp_write "$dir"
    done
    bash scripts/build/fixtures-build.sh freertos rust zenoh
    # 275-W1/#136 debt: prebuild the 6 standalone Entry demos at --release so
    # `freertos_run_plan_runtime.rs` LOCATES them instead of compiling in-test
    # (CLAUDE.md "no compilation inside tests"). Each Entry's `.cargo/config.toml`
    # now carries the `NROS_PLATFORM_*` env the board build.rs needs, so they
    # build standalone; --release (built-in profile) is what the run-plan reads
    # (a debug zenoh-pico is too slow to handshake on the emulated Cortex-M3).
    for entry in talker-entry listener-entry service-server-entry \
        service-client-entry action-server-entry action-client-entry; do
        ( cd "examples/qemu-arm-freertos/rust/$entry" && cargo build --release --quiet )
    done
    bash scripts/build/workspace-fixtures-build.sh freertos rust
    # phase-263 C2b — the embedded C workspace LAUNCH entry (issue 0097), consumed by
    # tests/c_freertos_entry_e2e.rs. Own build-subdir (one board per CMake configure).
    bash scripts/build/workspace-fixtures-build.sh freertos c
    # phase-263 C2c — the embedded C++ workspace LAUNCH entry, consumed by
    # tests/cpp_freertos_entry_e2e.rs.
    bash scripts/build/workspace-fixtures-build.sh freertos cpp
    # phase-263 C2c — the embedded MIXED (C+C++ + no_std Rust) workspace LAUNCH entry,
    # consumed by tests/mixed_freertos_entry_e2e.rs.
    bash scripts/build/workspace-fixtures-build.sh freertos mixed
    echo "FreeRTOS QEMU examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
[group("full-matrix")]
build-fixtures: build-examples build-fixture-extras
    @echo "FreeRTOS test fixtures built."

# Internal Phase 178.B target: only fixture artifacts not already covered by
# `build-examples`.
[group("examples")]
build-fixture-extras:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    # Phase 214.J.2 — codegen-stamp helpers for `ws sync` invocations below.
    source scripts/build/codegen-stamp.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    nros_cli="$(nros_cli_bin)"
    # Phase 214.I.2 — fail-loud prereq guard: the cyclone-fixture loop
    # below invokes `nros ws sync`. Probe once; emit `[PREREQ]` + exit 0
    # rather than burying the build in clap "unrecognized subcommand 'ws'"
    # stack traces (shipped 0.3.7 release predates the verb).
    nros_require_ws_sync "$nros_cli"
    export cargo_profile_args
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "FreeRTOS skip: arm-none-eabi-gcc not found"; exit 0
    fi
    echo "Building FreeRTOS test fixtures..."
    # Phase 176 — cmake parallelism via the fifo jobserver when present.
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then unset CMAKE_BUILD_PARALLEL_LEVEL; else export CMAKE_BUILD_PARALLEL_LEVEL="${NROS_BUILD_JOBS:-8}"; fi
    # The normal Rust role examples are produced by `build-examples`.
    # Phase 88.15.b — nros-log severity-coverage smoke fixture, selected by
    # stable manifest id so `build-examples` stays limited to role examples.
    bash scripts/build/fixtures-build.sh freertos rust --id freertos-logging-smoke-mps2

    toolchain="$(pwd)/cmake/toolchain/arm-freertos-armcm3.cmake"
    # Phase 140 removed the `build/install/bin/nros-codegen` install
    # layout. Cross builds (FreeRTOS) can't build the host codegen
    # tool with their cross toolchain, so build it once with the host
    # cargo and pass the absolute path via `-D_NANO_ROS_CODEGEN_TOOL`.
    echo "  → host nros-codegen (cargo)"
    nros_cargo_ensure_codegen_c
    codegen="$(nros_cargo_codegen_c_bin)"
    # Phase 186 — RMW availability follows config, fully in CMake. The example's
    # nros_provide_cyclonedds() self-provisions Cyclone from the pinned submodule
    # (sccache-accelerated) — no prebuilt install, no `just cyclonedds
    # *-cross-probe` pre-step. The ARM cross toolchain is already guaranteed here
    # (the recipe exited above if `arm-none-eabi-gcc` was missing). idlc resolves
    # host-side via find_program (Phase 186.3).
    #
    # Phase 177.38 — bake this platform's Cyclone role-set onto a distinct ROS
    # domain so its fixtures never collide with other Cyclone fixtures on the
    # derived RTPS ports. talker+listener share (they communicate). freertos=60.
    export NROS_DOMAIN_ID=60
    # Cyclone descriptor codegen needs a host `idlc` (find_program, Phase 186.3);
    # skip the cyclonedds fixtures gracefully when it's absent rather than failing
    # the whole fixture build — mirrors threadx-linux. The zenoh c/cpp/rust
    # fixtures (the primary coverage) are unaffected. Install a ROS 2 / CycloneDDS
    # idlc (or set IDLC_EXECUTABLE) to exercise the Cyclone variants.
    # Phase 220.C (Path B) — FreeRTOS rust cyclonedds fixtures are retired
    # from the cmake/corrosion path. Phase 212.M.5.b deleted the per-example
    # CMakeLists.txt; Phase 214.S.5.b (BSP gate — cyclonedds-sys vendored
    # build against the ARM cross toolchain + FreeRTOS POSIX shim) is the
    # remaining blocker for a pure-cargo cyclonedds path here. Until that
    # lands, the freertos rust cyclonedds e2e tests stay `#[ignore]`d
    # (see packages/testing/nros-tests/tests/freertos_qemu.rs) and the
    # build step is a no-op. The c/cpp cyclonedds cells on freertos are
    # also out of scope (none exist today — see examples/fixtures.toml).
    if command -v idlc >/dev/null 2>&1 || [ -x build/cyclonedds/bin/idlc ]; then
      echo "  → freertos rust cyclonedds fixtures retired (Phase 220.C path B — pure-cargo only; BSP gate 214.S.5.b)"
    else
      echo "  → freertos cyclonedds fixtures skipped (no idlc on PATH — install a ROS 2 / CycloneDDS idlc, or set IDLC_EXECUTABLE)"
    fi
    # Phase 181.5.b — collapsed-shape FreeRTOS C / C++ examples from the SSOT
    # manifest (examples/fixtures.toml). Zenoh-only (no DDS C/C++ siblings
    # existed pre-collapse on FreeRTOS). Per-cell RMW + build-subdir come from
    # the manifest; the platform-wide toolchain / codegen-tool -D defs are
    # injected via the shared builder's NROS_CMAKE_EXTRA_DEFS contract.
    export NROS_CMAKE_EXTRA_DEFS="-DCMAKE_TOOLCHAIN_FILE=$toolchain -DCMAKE_BUILD_TYPE=Release -D_NANO_ROS_CODEGEN_TOOL=$codegen"
    bash scripts/build/fixtures-build.sh freertos c zenoh
    bash scripts/build/fixtures-build.sh freertos cpp zenoh
    unset NROS_CMAKE_EXTRA_DEFS

# Build every FreeRTOS tier: core libraries, examples, and test fixtures.
[group("full-matrix")]
build-all: build-fixtures
    @echo "FreeRTOS build-all completed."

# Run FreeRTOS QEMU integration tests
[group("main")]
test verbose="": build-fixtures
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just setup-freertos"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just setup-freertos"
        exit 1
    fi
    just _nextest-platform freertos_qemu '{{verbose}}'

# Run all FreeRTOS tests including networked E2E
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just freertos setup"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just freertos setup"
        exit 1
    fi
    # Build tests
    args=(-p nros-tests --test freertos_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"
    # E2E networking tests (pubsub, service, action across RTOS platforms)
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(Freertos)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${e2e_args[@]}"

# Phase 121.6 — build the FreeRTOS Posix-port smoke test for
# nros-platform-freertos. Uses the in-tree FreeRTOS-Kernel + a
# minimal FreeRTOSConfig.h. Excludes net.c (no lwIP in this harness).
[group("examples")]
build-c-port:
    #!/usr/bin/env bash
    set -e
    cd tests/freertos-c-smoke
    cmake -B build >/dev/null
    cmake --build build

# Phase 121.6 — run the FreeRTOS Posix-port smoke test on the host.
# Boots FreeRTOS, runs the smoke task, exits with status 0 on PASS.
[group("debug")]
test-c-port: build-c-port
    #!/usr/bin/env bash
    set -e
    timeout 15 ./tests/freertos-c-smoke/build/nros_freertos_c_smoke

# CI: check prerequisites + test
[group("full-matrix")]
ci: doctor test

# Clean FreeRTOS build artifacts
[group("main")]
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-freertos-armcm3-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-freertos/rust/$example/target-zenoh"
        # Phase 220.C path B — `build-cyclonedds` retired from rust freertos
        # (no cmake-bridge build path); leave the rm for legacy artifacts.
        rm -rf "examples/qemu-arm-freertos/rust/$example/build-cyclonedds"
        rm -rf "examples/qemu-arm-freertos/c/$example/build-zenoh"
        rm -rf "examples/qemu-arm-freertos/cpp/$example/build-zenoh"
    done
    echo "FreeRTOS build artifacts cleaned"

# =============================================================================
# Tracing (Tonbandgeraet → Perfetto)
# =============================================================================

# Build and run a FreeRTOS example with tracing, then convert to Perfetto.
#
# Each example gets its own subdirectory so multiple instances can run in
# parallel (e.g., talker + listener with a shared zenohd).
#
# Usage:
#   just freertos trace talker    # single example
#   # For a pair, start zenohd first, then run in separate terminals:
#   #   terminal 1: zenohd -l tcp/127.0.0.1:7451 --no-multicast-scouting
#   #   terminal 2: just freertos trace talker
#   #   terminal 3: just freertos trace listener
#
# Output: test-logs/freertos-trace/<example>/trace.pf
trace example="talker":
    #!/usr/bin/env bash
    set -e

    EXAMPLE="{{example}}"
    EXAMPLE_DIR="examples/qemu-arm-freertos/rust/$EXAMPLE"
    TRACE_DIR="test-logs/freertos-trace/$EXAMPLE"
    mkdir -p "$TRACE_DIR"

    if [ ! -d "$EXAMPLE_DIR" ]; then
        echo "ERROR: Example not found: $EXAMPLE_DIR"
        exit 1
    fi

    echo "=== Building $EXAMPLE with tracing ==="
    (cd "$EXAMPLE_DIR" && NROS_TRACE=1 cargo build --release --target-dir target-zenoh)

    # Find the binary name from Cargo.toml
    BINARY=$(grep '^name = ' "$EXAMPLE_DIR/Cargo.toml" | head -1 | sed 's/name = "\(.*\)"/\1/')
    BINARY_PATH="$EXAMPLE_DIR/target-zenoh/thumbv7m-none-eabi/release/$BINARY"
    if [ ! -f "$BINARY_PATH" ]; then
        echo "ERROR: Binary not found: $BINARY_PATH"
        exit 1
    fi

    # Build tband-cli if needed (before cd to trace dir)
    TBAND_CLI="third-party/tracing/Tonbandgeraet/tools/target/release/tband-cli"
    if [ ! -f "$TBAND_CLI" ]; then
        echo "Building tband-cli..."
        (cd "third-party/tracing/Tonbandgeraet/tools" && cargo build --release -p tband-cli)
    fi

    echo "=== Running $EXAMPLE in QEMU (with tracing) ==="
    echo "    Trace will be dumped to $TRACE_DIR/trace.bin on exit."
    # Run QEMU — semihosting creates trace.bin in CWD
    TRACE_DIR_ABS="$(pwd)/$TRACE_DIR"
    BINARY_ABS="$(pwd)/$BINARY_PATH"
    TBAND_CLI_ABS="$(pwd)/$TBAND_CLI"
    cd "$TRACE_DIR_ABS"
    rm -f trace.bin trace.pf

    # Run QEMU with a trap so Ctrl-C kills it cleanly.
    # QEMU with semihosting ignores SIGINT — we must kill explicitly.
    QEMU_PID=""
    trap 'if [ -n "$QEMU_PID" ]; then kill $QEMU_PID 2>/dev/null; wait $QEMU_PID 2>/dev/null; fi; exit 130' INT TERM
    timeout 60 {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} \
        -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "$BINARY_ABS" \
        -nic user,model=lan9118 &
    QEMU_PID=$!
    wait $QEMU_PID || true
    QEMU_PID=""
    trap - INT TERM

    if [ ! -f trace.bin ]; then
        echo "ERROR: trace.bin not created — semihosting file I/O may not be working"
        exit 1
    fi

    echo "=== Converting trace to Perfetto format ==="
    "$TBAND_CLI_ABS" conv --format bin --mode free-rtos --core-count 1 \
        --output trace.pf trace.bin

    echo ""
    echo "=== Trace captured ==="
    echo "  Raw:      $TRACE_DIR/trace.bin ($(wc -c < trace.bin) bytes)"
    echo "  Perfetto: $TRACE_DIR/trace.pf"
    echo ""
    echo "Open in browser: https://ui.perfetto.dev"
    echo "  → Open trace file → select $TRACE_DIR/trace.pf"

# =============================================================================
# Run examples (build + launch QEMU MPS2-AN385 with slirp networking)
#
# QEMU with semihosting ignores SIGINT (Ctrl-C). Each recipe runs QEMU in
# the background and traps INT/TERM to kill it explicitly.
# =============================================================================

[no-exit-message]
_run-qemu example binary:
    #!/usr/bin/env bash
    set -e
    cd "examples/qemu-arm-freertos/rust/{{example}}" && \
        cargo build --release --target-dir target-zenoh
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "target-zenoh/thumbv7m-none-eabi/release/{{binary}}" \
        -nic user,model=lan9118 &
    PID=$!
    wait $PID

# Run FreeRTOS talker on QEMU
talker: (_run-qemu "talker" "qemu-freertos-talker")

# Run FreeRTOS listener on QEMU
listener: (_run-qemu "listener" "qemu-freertos-listener")

# Run FreeRTOS service server on QEMU
service-server: (_run-qemu "service-server" "qemu-freertos-service-server")

# Run FreeRTOS service client on QEMU
service-client: (_run-qemu "service-client" "qemu-freertos-service-client")

# Run FreeRTOS action server on QEMU
action-server: (_run-qemu "action-server" "qemu-freertos-action-server")

# Run FreeRTOS action client on QEMU
action-client: (_run-qemu "action-client" "qemu-freertos-action-client")

# Start zenohd router for FreeRTOS QEMU examples (port 7451)
zenohd:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    exec "$(nros_zenohd_bin)" --listen tcp/127.0.0.1:7451 --no-multicast-scouting
