set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install NuttX ARM C/C++ libraries into build/install/
#
# DISABLED: NuttX C/C++ library cross-compilation requires build-std, which
# resolves libc from the sysroot's Cargo.lock (ignoring workspace [patch.crates-io]).
# The upstream libc crate (0.2.178) is missing _SC_HOST_NAME_MAX for NuttX,
# blocking std compilation for armv7a-nuttx-eabihf.
#
# Rust NuttX examples work because they use standalone Cargo.toml manifests
# with their own [patch.crates-io] that build-std does honor for non-workspace
# packages.
#
# To re-enable: either upstream _SC_HOST_NAME_MAX to rust-lang/libc, or wait
# for a nightly that includes it.
install:
    @echo "NuttX C/C++ libraries: skipped (upstream libc missing _SC_HOST_NAME_MAX)"
    @echo "Rust NuttX examples are unaffected — use 'just nuttx build' and 'just nuttx test'"

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
        packages/boards/nros-nuttx-qemu-arm/scripts/build-nuttx.sh

# Build NuttX QEMU ARM virt examples
build:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX QEMU examples (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    echo "Building NuttX QEMU ARM virt examples..."
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-arm-nuttx/rust/zenoh/$example && cargo +nightly build --release)
    done
    echo "NuttX QEMU examples built!"

# Run NuttX QEMU integration tests
test verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just nuttx setup"
        exit 1
    fi
    args=(-p nros-tests --test nuttx_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Build NuttX kernel (run after git submodule update --init)
setup: build-kernel
    @echo "NuttX setup complete!"

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
    CC_armv7a_nuttx_eabihf=arm-none-eabi-gcc cargo +nightly build --release
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    # -icount shift=auto: sync virtual clock with wall-clock for proper sleep/timeout
    # -device virtio-net-device: MMIO transport (NuttX virtio-net driver)
    qemu-system-arm -M virt -cpu cortex-a7 -nographic \
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

# Clean NuttX build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-nuttx-armv7a-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-nuttx/rust/zenoh/$example/target"
    done
    echo "NuttX build artifacts cleaned"
