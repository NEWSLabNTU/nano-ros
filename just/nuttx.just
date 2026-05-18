set working-directory := '..'

# Phase 143 — patched qemu-system-arm built by `just qemu setup-qemu`.
# Used by NuttX QEMU recipes so the `-netdev dgram,local.type=unix,…`
# multi-instance path works on Ubuntu jammy (system qemu 6.2 too old).
QEMU_BIN := absolute_path("build/qemu") / "bin/qemu-system-arm"

default:
    @just --list --justfile {{source_file()}}

# Phase 140 — `install` recipe deleted along with the install-local
# layout. NuttX examples consume nano-ros via `add_subdirectory(<repo>)`
# (Phase 144.6) or the `integrations/nuttx/` shell (Phase 139).

# Build NuttX kernel (requires arm-none-eabi-gcc + kconfig tools)
build-kernel:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX kernel build (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    if ! command -v arm-none-eabi-gcc &>/dev/null; then
        echo "Skipping NuttX kernel build (arm-none-eabi-gcc not found)"
        exit 0
    fi
    if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
        echo "Skipping NuttX kernel build (kconfig tools not found)"
        exit 0
    fi
    NUTTX_DIR=$NUTTX_DIR NUTTX_APPS_DIR=$NUTTX_APPS_DIR \
        packages/boards/nros-board-nuttx-qemu-arm/scripts/build-nuttx.sh

# Build NuttX core artifacts.
build: build-kernel
    @echo "NuttX core artifacts built."

# Build NuttX QEMU ARM virt examples
build-examples:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX QEMU examples (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    echo "Building NuttX QEMU ARM virt examples..."
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        # No +nightly override — the nightly version is pinned by
        # examples/qemu-arm-nuttx/rust-toolchain.toml (see comments there).
        (cd examples/qemu-arm-nuttx/rust/zenoh/$example && cargo build --release)
    done
    echo "NuttX QEMU examples built!"

# Build all example binaries used as test fixtures (Rust + C + C++).
build-fixtures:
    #!/usr/bin/env bash
    set -e
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "NuttX skip: arm-none-eabi-gcc not found"; exit 0
    fi
    if [ -z "${NUTTX_DIR:-}" ] && [ ! -d third-party/nuttx/nuttx ]; then
        echo "NuttX skip: NUTTX_DIR unset and third-party/nuttx/nuttx absent"; exit 0
    fi
    echo "Building NuttX test fixtures..."
    nuttx_dir="${NUTTX_DIR:-$(pwd)/third-party/nuttx/nuttx}"
    rust_examples=(talker listener service-server service-client action-server action-client)
    for ex in "${rust_examples[@]}"; do
        echo "  → nuttx/rust/$ex"
        ( cd "examples/qemu-arm-nuttx/rust/zenoh/$ex" && \
          CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
          cargo build --release --quiet )
    done
    # Phase 97.4.nuttx — DDS variants alongside zenoh.
    dds_examples=(talker listener)
    for ex in "${dds_examples[@]}"; do
        echo "  → nuttx/rust/dds/$ex"
        ( cd "examples/qemu-arm-nuttx/rust/dds/$ex" && \
          CC_armv7a_nuttx_eabi=arm-none-eabi-gcc \
          cargo build --release --quiet )
    done
    # Phase 140 — NuttX C/C++ examples consume nano-ros in-tree via
    # add_subdirectory; codegen tool must be built with the host
    # cargo (cross toolchain can't build it) and pointed at via
    # -D_NANO_ROS_CODEGEN_TOOL.
    # Phase 140 left the NuttX C/C++ cmake examples unable to build
    # against the cross toolchain: nros-c's `nros_config_generated.h`
    # is emitted by the (tier-2 host) Corrosion build of nros-c, which
    # can't be cross-built without `-Z build-std`, and the only
    # canonical NuttX entry point (`nros_board_link_app` →
    # `nros-nuttx-ffi` cargo build) doesn't expose the header to a
    # sibling cmake project. Re-wiring requires extracting the size
    # probe into a standalone codegen step — tracked as a follow-up.
    # Rust nuttx examples above are unaffected (they consume
    # nros-nuttx-ffi via cargo directly).
    echo "  ↷ nuttx C/C++ cmake examples skipped — see Phase 140 follow-up"

# Build every NuttX tier: core artifacts, examples, and test fixtures.
build-all: build build-examples build-fixtures
    @echo "NuttX build-all completed."

# Run NuttX QEMU integration tests
test verbose="": build-examples
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just nuttx setup"
        exit 1
    fi
    just _nextest-platform nuttx_qemu '{{verbose}}'

# Run all NuttX tests including networked E2E
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test nuttx_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"
    e2e_args=(-p nros-tests --test rtos_e2e -E 'test(Nuttx)' --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        e2e_args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${e2e_args[@]}"

# CI: check prerequisites + test
ci: doctor test

# Install NuttX prerequisites: initialize submodules + build kernel.
# Phase 123.A.8 — submodule fetch delegated to tools/setup.sh; kernel
# build stays here (NuttX-specific make invocation).
setup:
    #!/usr/bin/env bash
    set -e
    "{{justfile_directory()}}/tools/setup.sh" --platform=nuttx --skip-apt-check
    just nuttx build-kernel
    echo "NuttX setup complete!"

# Diagnose NuttX install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$NUTTX_DIR/arch" ]; then
        echo "  [OK] NuttX source: $NUTTX_DIR"
    else
        echo "  [MISSING] NuttX source at $NUTTX_DIR (run: just nuttx setup)"
        fail=1
    fi
    if [ -d "$NUTTX_APPS_DIR" ]; then
        echo "  [OK] NuttX apps: $NUTTX_APPS_DIR"
    else
        echo "  [MISSING] NuttX apps at $NUTTX_APPS_DIR (run: just nuttx setup)"
        fail=1
    fi
    if [ -f "$NUTTX_DIR/include/nuttx/config.h" ]; then
        echo "  [OK] NuttX kernel configured"
    else
        echo "  [MISSING] NuttX kernel config ($NUTTX_DIR/include/nuttx/config.h) (run: just nuttx setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null; then
        echo "  [OK] kconfig tools"
    else
        echo "  [MISSING] kconfig-frontends-nox (run: just workspace apt-packages)"
        fail=1
    fi
    # Phase 127.B.5 — NuttX DDS RTPS multi-instance tests rely on
    # mcast-style cross-process delivery between two QEMU processes.
    # QEMU 6.2's `socket,mcast=` cannot do this reliably; the
    # documented workaround uses `-netdev dgram,local.type=unix,...`,
    # which was introduced in QEMU 7.2.
    # Phase 143 — patched build at {{QEMU_BIN}} is the primary path
    # picked up automatically by `nros_tests::qemu::qemu_system_arm_path`.
    if [ -x {{QEMU_BIN}} ]; then
        ver=$({{QEMU_BIN}} --version 2>/dev/null | head -1)
        echo "  [OK] patched qemu-system-arm: {{QEMU_BIN}} ($ver)"
    elif command -v qemu-system-arm &>/dev/null; then
        ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
        major=${ver%%.*}
        minor=${ver##*.}
        if [ -n "$ver" ] && { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
            echo "  [OK] system qemu-system-arm ($ver, fallback path; >= 7.2 — supports -netdev dgram unix)"
        else
            echo "  [WARN] system qemu-system-arm ($ver) < 7.2 AND patched build absent —"
            echo "         NuttX DDS multi-instance tests need '-netdev dgram,local.type=unix,...'"
            echo "         which was introduced in QEMU 7.2."
            echo ""
            echo "         Primary remedy (no sudo, portable): just qemu setup-qemu"
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
        echo "  [MISSING] qemu-system-arm — run \`just qemu setup-qemu\` (recommended)"
        echo "            or \`just workspace apt-packages\` for the system binary."
    fi
    exit $fail

# =============================================================================
# Run examples (build + launch QEMU ARM virt with slirp networking)
#
# NOTE: NuttX interactive runs are experimental. The zenoh-pico session may
# fail to connect on first boot. The automated E2E tests (just nuttx test)
# handle this with proper timing. For interactive use, start zenohd first
# and allow a few retries.
#
# QEMU ignores SIGINT — each recipe runs QEMU in the background and
# traps INT/TERM to kill it explicitly (Ctrl-C works).
# =============================================================================

[no-exit-message]
_run-qemu example binary:
    #!/usr/bin/env bash
    set -e
    cd "examples/qemu-arm-nuttx/rust/zenoh/{{example}}"
    # Nightly version pinned by examples/qemu-arm-nuttx/rust-toolchain.toml.
    CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc cargo build --release
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    # -icount shift=auto: sync virtual clock with wall-clock for proper sleep/timeout
    # -device virtio-net-device: MMIO transport (NuttX virtio-net driver)
    {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -M virt -cpu cortex-a7 -nographic \
        -icount shift=auto \
        -kernel "target/armv7a-nuttx-eabihf/release/{{binary}}" \
        -netdev user,id=net0 -device virtio-net-device,netdev=net0 &
    PID=$!
    wait $PID

# Run NuttX talker on QEMU
talker: (_run-qemu "talker" "nuttx-rs-talker")

# Run NuttX listener on QEMU
listener: (_run-qemu "listener" "nuttx-rs-listener")

# Run NuttX service server on QEMU
service-server: (_run-qemu "service-server" "nuttx-rs-service-server")

# Run NuttX service client on QEMU
service-client: (_run-qemu "service-client" "nuttx-rs-service-client")

# Run NuttX action server on QEMU
action-server: (_run-qemu "action-server" "nuttx-rs-action-server")

# Run NuttX action client on QEMU
action-client: (_run-qemu "action-client" "nuttx-rs-action-client")

# Start zenohd router for NuttX QEMU examples (port 7452)
# Start zenohd router for NuttX QEMU examples (port 7452)
# Must listen on 0.0.0.0 for virtio-net-device + slirp gateway (10.0.2.2)
zenohd:
    build/zenohd/zenohd --listen tcp/0.0.0.0:7452 --no-multicast-scouting

# Phase 127.B.5 — one-time host setup for NuttX DDS cross-QEMU mcast.
#
# QEMU's `-netdev socket,mcast=…,localaddr=127.0.0.1` puts the mcast
# egress on `lo`, but the kernel only loops frames back to a sibling
# QEMU when a route for the group is present on `lo`. Without these
# routes, two local NuttX QEMU instances silently fail to exchange
# RTPS SPDP frames and the DDS listener receives 0 messages.
#
# Idempotent — re-running drops + re-adds the routes. Requires sudo.
setup-mcast-route:
    #!/usr/bin/env bash
    set -e
    echo "Adding mcast loopback routes (requires sudo)..."
    sudo ip route del 230.10.0.0/16 dev lo 2>/dev/null || true
    sudo ip route del 239.0.0.0/8   dev lo 2>/dev/null || true
    sudo ip route add 230.10.0.0/16 dev lo
    sudo ip route add 239.0.0.0/8   dev lo
    echo "Routes added:"
    ip route show 230.10.0.0/16
    ip route show 239.0.0.0/8

# Clean NuttX build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-nuttx-armv7a-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-nuttx/rust/zenoh/$example/target"
    done
    echo "NuttX build artifacts cleaned"
