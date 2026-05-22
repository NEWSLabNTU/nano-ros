set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
LOG_DIR := "test-logs"

# Project-local patched QEMU prefix (built by `just qemu setup-qemu`).
QEMU_PREFIX := absolute_path("build/qemu")
QEMU_BIN := QEMU_PREFIX / "bin/qemu-system-arm"

# Build a project-local patched qemu-system-arm under build/qemu.
#
# Pulls third-party/qemu/qemu (stable-11.0), applies every patch in
# third-party/qemu/patches/, configures for the arm-softmmu target
# (no other targets, no docs, no tools), and installs into
# build/qemu/. Subsequent runs are no-ops unless the patches change
# (touch a patch to force a rebuild) — wire the test runner with
# QEMU_SYSTEM_ARM=$(just qemu qemu-bin) to pick this up.
setup-qemu:
    #!/usr/bin/env bash
    set -euo pipefail
    submodule_path=third-party/qemu/qemu
    prefix={{QEMU_PREFIX}}
    bin={{QEMU_BIN}}
    if [ ! -d "$submodule_path/.git" ] && [ ! -f "$submodule_path/.git" ]; then
        git submodule update --init --depth=1 -- "$submodule_path"
    fi
    # Re-stamp = submodule HEAD commit. Skip rebuild if binary newer
    # than the recorded submodule commit's mtime (a proxy for "no
    # source change since last build").
    if [ -x "$bin" ]; then
        head_stamp="$submodule_path/.git/HEAD"
        if [ ! -f "$head_stamp" ] || [ "$bin" -nt "$head_stamp" ]; then
            echo "qemu: $bin up to date"
            "$bin" --version | head -1
            exit 0
        fi
    fi
    echo "qemu: building (this can take ~10 min)"
    # Patches live in NEWSLabNTU/qemu fork's git history (branch
    # `nano-ros-v11.0.0-patches`). The submodule checkout already
    # has them applied — no separate `git apply` step.
    pushd "$submodule_path" >/dev/null
    mkdir -p build
    cd build
    ../configure \
        --target-list=arm-softmmu \
        --disable-docs \
        --disable-tools \
        --disable-gtk \
        --disable-vnc \
        --disable-sdl \
        --disable-spice \
        --enable-slirp \
        --prefix="$prefix" \
        >/dev/null
    make -j$(nproc) >/dev/null
    make install >/dev/null
    popd >/dev/null
    echo "qemu: installed to $bin"
    "$bin" --version | head -1

# Print the path to the patched qemu-system-arm binary. Empty if
# `just qemu setup-qemu` hasn't been run.
qemu-bin:
    @echo {{QEMU_BIN}}

# Run all qemu-baremetal tests with the patched qemu-system-arm.
# Wraps `just qemu test` with QEMU_SYSTEM_ARM exported so the test
# runner picks the project-local build instead of system qemu.
test-patched verbose="":
    @just qemu setup-qemu
    QEMU_SYSTEM_ARM={{QEMU_BIN}} just qemu test {{verbose}}

# Build QEMU ARM bare-metal examples
build:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    echo "Building QEMU ARM examples..."
    for toml in $(find examples/qemu-arm-baremetal -mindepth 3 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        dir="$(dirname "$toml")"
        echo "  build $dir"
        (cd "$dir" && cargo build $cargo_profile_args)
    done
    # Phase 131.B — bench/test-fixture binaries that used to live under
    # examples/qemu-arm-baremetal/rust/{core,standalone,zenoh/large-msg-test}
    # now sit under packages/testing/.
    for dir in \
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu \
        packages/testing/nros-tests/bins/lan9118-qemu \
        packages/testing/nros-tests/bins/logging-smoke-mps2-baremetal \
        packages/testing/nros-bench/wcet-cycles-qemu \
        packages/testing/nros-bench/large-msg-baremetal; do
        echo "  build $dir"
        (cd "$dir" && cargo build $cargo_profile_args)
    done
    # Also build qemu-smoltcp-bridge (library in packages/reference/)
    (cd packages/reference/qemu-smoltcp-bridge && cargo build $cargo_profile_args)

# Build all example binaries used as test fixtures (bare-metal Rust).
# Skips cleanly if the ARM cross-toolchain is missing.
build-fixtures:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "QEMU bare-metal skip: arm-none-eabi-gcc not found"; exit 0
    fi
    echo "Building QEMU bare-metal test fixtures..."
    # Phase 160 follow-up — parallel cargo builds. Each example has its
    # own isolated `target/`; no cross-fixture lock contention. Cap
    # concurrent cargos at 8 so the per-cargo job server (default nproc)
    # × 8 stays in the same ballpark as nproc on a 32-core host without
    # over-saturating smaller hosts.
    dirs=()
    while IFS= read -r toml; do
        dirs+=("$(dirname "$toml")")
    done < <(find examples/qemu-arm-baremetal -mindepth 3 -name Cargo.toml \
                  -not -path '*/target/*' -not -path '*/generated/*' | sort)
    # Phase 131.B — relocated bench/test-fixture crates.
    dirs+=(
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu
        packages/testing/nros-tests/bins/lan9118-qemu
        packages/testing/nros-tests/bins/logging-smoke-mps2-baremetal
        packages/testing/nros-bench/wcet-cycles-qemu
        packages/testing/nros-bench/large-msg-baremetal
        packages/reference/qemu-smoltcp-bridge
    )
    printf '%s\n' "${dirs[@]}" | parallel --jobs "$cargo_frontends" --halt now,fail=1 --line-buffer \
        'echo "  → {}" && cd {} && cargo build $cargo_profile_args --quiet'

# Run all QEMU tests (non-networked)
test verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just init-test-logs
    just qemu test-basic {{verbose}} || failed=1
    just qemu test-wcet {{verbose}} || failed=1
    just qemu test-lan9118 {{verbose}} || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some QEMU tests failed."
        exit 1
    else
        echo "All QEMU tests passed!"
    fi

# Run basic QEMU test (nros serialization on Cortex-M3)
test-basic verbose="": build
    ./tests/run-test.sh --name qemu-basic --log {{LOG_DIR}}/latest/qemu-basic.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel packages/testing/nros-tests/bins/cdr-roundtrip-qemu/target/thumbv7m-none-eabi/release/qemu-rs-test

# Run WCET benchmark on QEMU (DWT cycle counter)
test-wcet verbose="": build
    ./tests/run-test.sh --name qemu-wcet-bench --log {{LOG_DIR}}/latest/qemu-wcet-bench.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel packages/testing/nros-bench/wcet-cycles-qemu/target/thumbv7m-none-eabi/release/qemu-rs-wcet-bench

# Run LAN9118 Ethernet driver test (mps2-an385)
test-lan9118 verbose="": build
    ./tests/run-test.sh --name qemu-lan9118 --log {{LOG_DIR}}/latest/qemu-lan9118.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine mps2-an385 -nographic \
            -semihosting-config enable=on,target=native \
            -kernel packages/testing/nros-tests/bins/lan9118-qemu/target/thumbv7m-none-eabi/release/qemu-rs-lan9118

# Run all QEMU tests including networked E2E and RTIC
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    just build-zenohd
    args=(-p nros-tests --no-fail-fast \
          -E 'binary(emulator) or binary(large_msg)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${nextest_profile_args[@]}" "${args[@]}"

# Check if QEMU is installed
check:
    @which qemu-system-arm > /dev/null || (echo "Error: qemu-system-arm not found. Install with: sudo apt install qemu-system-arm" && exit 1)
    @echo "QEMU ARM is installed"

# CI: check + test
ci: check test

# Setup QEMU TAP network bridge (requires sudo)
setup-network:
    sudo ./scripts/qemu/setup-network.sh

# Teardown QEMU TAP network bridge (requires sudo)
teardown-network:
    sudo ./scripts/qemu/setup-network.sh --down

# Show QEMU network status
status-network:
    ./scripts/qemu/setup-network.sh --status

# Test QEMU zenoh-pico communication (requires zenohd + TAP network)
test-zenoh: build
    #!/usr/bin/env bash
    set -e
    echo "============================================"
    echo "  QEMU zenoh-pico Communication Test"
    echo "============================================"
    echo ""
    echo "Prerequisites:"
    echo "  1. TAP network: sudo ./scripts/qemu/setup-network.sh"
    echo "  2. zenohd running: zenohd --listen tcp/0.0.0.0:7447"
    echo ""
    echo "This test requires QEMU 7.0+ for reliable TAP networking."
    echo "Ubuntu 22.04 ships QEMU 6.2.0 which has TAP issues."
    echo ""
    echo "Current QEMU version:"
    qemu-system-arm --version | head -1
    echo ""
    echo "To run manually:"
    echo "  Terminal 1: zenohd --listen tcp/0.0.0.0:7447"
    echo "  Terminal 2: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary examples/qemu-arm-baremetal/rust/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker"
    echo "  Terminal 3: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 --binary examples/qemu-arm-baremetal/rust/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener"
    echo ""
    echo "Binaries built at:"
    echo "  examples/qemu-arm-baremetal/rust/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker"
    echo "  examples/qemu-arm-baremetal/rust/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener"
    echo ""
    echo "Note: Automated test not yet implemented (requires QEMU 7.0+)"
    echo "============================================"

# Launch QEMU mps2-an385 with networking (example)
run-networked TAP="tap-qemu0" IP="192.0.2.10" BINARY="":
    ./scripts/qemu/launch-mps2-an385.sh --tap {{TAP}} --ip {{IP}} --binary {{BINARY}}

# Install QEMU bare-metal prerequisites (ARM zenoh-pico + patched
# qemu-system-arm under build/qemu/). Building patched QEMU is
# idempotent and short-circuits when build/qemu/bin/qemu-system-arm
# is newer than every patch file.
setup: build-zenoh-pico setup-qemu

# Diagnose QEMU bare-metal install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -f build/qemu-zenoh-pico/libzenohpico.a ]; then
        echo "  [OK] qemu-zenoh-pico: build/qemu-zenoh-pico/libzenohpico.a"
    else
        echo "  [MISSING] qemu-zenoh-pico (run: just qemu setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if [ -x {{QEMU_BIN}} ]; then
        ver=$({{QEMU_BIN}} --version 2>/dev/null | head -1)
        echo "  [OK] patched qemu-system-arm: {{QEMU_BIN}} ($ver)"
    else
        echo "  [MISSING] patched qemu-system-arm (run: just qemu setup-qemu)"
        fail=1
    fi
    if command -v qemu-system-arm &>/dev/null; then
        ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
        major=${ver%%.*}
        minor=${ver##*.}
        # Phase 127.B.5 — `-netdev dgram,local.type=unix,...` (the
        # supported workaround for QEMU 6.2's broken cross-process
        # `socket,mcast=` delivery) was introduced in QEMU 7.2.
        # NuttX DDS / RTPS multi-instance tests need it.
        if [ -n "$ver" ] && { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
            echo "  [OK] system qemu-system-arm ($ver, fallback path; >= 7.2 — supports -netdev dgram unix)"
        elif [ -x {{QEMU_BIN}} ]; then
            # Phase 143 — patched build is the primary path and is
            # already OK above; demote the system-binary warning to
            # an informational note so users aren't pressured to
            # touch system packages.
            echo "  [INFO] system qemu-system-arm ($ver) < 7.2 — patched build at {{QEMU_BIN}} is the active path; nothing to do."
        else
            echo "  [WARN] system qemu-system-arm ($ver) < 7.2 AND patched build absent."
            echo "         Primary remedy (no sudo, portable across distros):"
            echo ""
            echo "           just qemu setup-qemu"
            echo ""
            echo "         (builds third-party/qemu/qemu @ stable-11.0 with the"
            echo "         LAN9118 RX-flush patch into build/qemu/bin/qemu-system-arm;"
            echo "         picked up automatically by the test harness via Phase 143's"
            echo "         qemu_system_arm_path() resolver — no env-var needed.)"
            echo ""
            echo "         Fallback (system-wide, requires sudo):"
            if [ -f /etc/os-release ] && grep -q '^ID=ubuntu' /etc/os-release; then
                codename=$(. /etc/os-release; echo "$VERSION_CODENAME")
                echo "           Ubuntu detected ($codename). Canonical server-backports PPA:"
                echo "             sudo add-apt-repository ppa:canonical-server/server-backports"
                echo "             sudo apt update && sudo apt install qemu-system-arm"
                echo "           https://launchpad.net/~canonical-server/+archive/ubuntu/server-backports"
            else
                echo "           Build from source: https://www.qemu.org/download/#source"
            fi
        fi
    else
        echo "  [MISSING] system qemu-system-arm (apt path). The patched"
        echo "            build at {{QEMU_BIN}} is the recommended path —"
        echo "            run \`just qemu setup-qemu\` to produce it."
        # No fail bump: if the patched build is in place, the system
        # binary is unused. The patched-binary check above already
        # bumped fail when {{QEMU_BIN}} was absent.
    fi
    exit $fail

# Build zenoh-pico for ARM Cortex-M3 (required for QEMU examples)
build-zenoh-pico:
    @./scripts/qemu/build-zenoh-pico.sh

# Clean zenoh-pico ARM build
clean-zenoh-pico:
    ./scripts/qemu/build-zenoh-pico.sh --clean

# Show QEMU help
help:
    @echo "QEMU Bare-Metal Testing"
    @echo "======================="
    @echo ""
    @echo "Prerequisites:"
    @echo "  1. Install QEMU: sudo apt install qemu-system-arm"
    @echo "  2. Set up network: sudo ./scripts/qemu/setup-network.sh"
    @echo ""
    @echo "Build & Test (non-networked):"
    @echo "  just qemu build              # Build all QEMU examples"
    @echo "  just qemu test               # Run all QEMU tests (no network)"
    @echo "  just qemu test-basic         # Run basic serialization test"
    @echo "  just qemu test-wcet          # Run WCET benchmark"
    @echo "  just qemu test-lan9118       # Run LAN9118 driver test"
    @echo ""
    @echo "Zenoh-pico (networked):"
    @echo "  just qemu build-zenoh-pico   # Build zenoh-pico for ARM Cortex-M3"
    @echo "  just qemu test-zenoh         # Show zenoh test instructions"
    @echo ""
    @echo "Network Setup:"
    @echo "  just qemu setup-network      # Create TAP bridge (requires sudo)"
    @echo "  just qemu teardown-network   # Remove TAP bridge (requires sudo)"
    @echo "  just qemu status-network     # Show network status"
    @echo ""
    @echo "Manual QEMU Launch:"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --binary app.elf"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary app.elf"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --gdb --binary app.elf"
    @echo ""
    @echo "For more details: ./scripts/qemu/launch-mps2-an385.sh --help"

# Clean QEMU bare-metal build artifacts
clean: clean-zenoh-pico
    #!/usr/bin/env bash
    for toml in $(find examples/qemu-arm-baremetal -mindepth 3 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        rm -rf "$(dirname "$toml")/target"
    done
    # Phase 131.B — relocated bare-metal bench/fixture crates.
    for dir in \
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu \
        packages/testing/nros-tests/bins/lan9118-qemu \
        packages/testing/nros-tests/bins/logging-smoke-mps2-baremetal \
        packages/testing/nros-bench/wcet-cycles-qemu \
        packages/testing/nros-bench/large-msg-baremetal; do
        rm -rf "$dir/target"
    done
    rm -rf packages/reference/qemu-smoltcp-bridge/target
    echo "QEMU bare-metal build artifacts cleaned"

# =============================================================================
# Run examples (build + launch QEMU MPS2-AN385 with slirp networking)
# =============================================================================

# QEMU ignores SIGINT — run in background with trap for Ctrl-C
[no-exit-message]
_run-qemu-mps2 binary: build
    #!/usr/bin/env bash
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "{{binary}}" \
        -nic user,model=lan9118 &
    PID=$!
    wait $PID

# Run bare-metal talker on QEMU
talker: (_run-qemu-mps2 "examples/qemu-arm-baremetal/rust/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker")

# Run bare-metal listener on QEMU
listener: (_run-qemu-mps2 "examples/qemu-arm-baremetal/rust/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener")

# Run RTIC service server on QEMU
rtic-service-server: (_run-qemu-mps2 "examples/qemu-arm-baremetal/rust/service-server-rtic/target/thumbv7m-none-eabi/release/qemu-bsp-rtic-service-server")

# Run RTIC service client on QEMU
rtic-service-client: (_run-qemu-mps2 "examples/qemu-arm-baremetal/rust/service-client-rtic/target/thumbv7m-none-eabi/release/qemu-bsp-rtic-service-client")

# Run RTIC action server on QEMU
rtic-action-server: (_run-qemu-mps2 "examples/qemu-arm-baremetal/rust/action-server-rtic/target/thumbv7m-none-eabi/release/qemu-bsp-rtic-action-server")

# Run RTIC action client on QEMU
rtic-action-client: (_run-qemu-mps2 "examples/qemu-arm-baremetal/rust/action-client-rtic/target/thumbv7m-none-eabi/release/qemu-bsp-rtic-action-client")

# Start zenohd router for bare-metal QEMU examples (port 7450)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7450 --no-multicast-scouting
