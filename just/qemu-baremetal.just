set working-directory := '..'
LOG_DIR := "test-logs"

# Build QEMU ARM bare-metal examples
build:
    #!/usr/bin/env bash
    set -e
    echo "Building QEMU ARM examples..."
    for toml in $(find examples/qemu-arm-baremetal -mindepth 3 -name Cargo.toml \
                       -not -path '*/target/*' -not -path '*/generated/*' | sort); do
        dir="$(dirname "$toml")"
        echo "  build $dir"
        (cd "$dir" && cargo build --release)
    done
    # Also build qemu-smoltcp-bridge (library in packages/reference/)
    (cd packages/reference/qemu-smoltcp-bridge && cargo build --release)

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
        qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu-arm-baremetal/rust/core/cdr-test/target/thumbv7m-none-eabi/release/qemu-rs-test

# Run WCET benchmark on QEMU (DWT cycle counter)
test-wcet verbose="": build
    ./tests/run-test.sh --name qemu-wcet-bench --log {{LOG_DIR}}/latest/qemu-wcet-bench.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu-arm-baremetal/rust/core/wcet-bench/target/thumbv7m-none-eabi/release/qemu-rs-wcet-bench

# Run LAN9118 Ethernet driver test (mps2-an385)
test-lan9118 verbose="": build
    ./tests/run-test.sh --name qemu-lan9118 --log {{LOG_DIR}}/latest/qemu-lan9118.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
            -semihosting-config enable=on,target=native \
            -kernel examples/qemu-arm-baremetal/rust/standalone/lan9118/target/thumbv7m-none-eabi/release/qemu-rs-lan9118

# Check if QEMU is installed
check:
    @which qemu-system-arm > /dev/null || (echo "Error: qemu-system-arm not found. Install with: sudo apt install qemu-system-arm" && exit 1)
    @echo "QEMU ARM is installed"

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
    echo "  Terminal 2: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary examples/qemu-arm-baremetal/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker"
    echo "  Terminal 3: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 --binary examples/qemu-arm-baremetal/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener"
    echo ""
    echo "Binaries built at:"
    echo "  examples/qemu-arm-baremetal/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker"
    echo "  examples/qemu-arm-baremetal/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener"
    echo ""
    echo "Note: Automated test not yet implemented (requires QEMU 7.0+)"
    echo "============================================"

# Launch QEMU mps2-an385 with networking (example)
run-networked TAP="tap-qemu0" IP="192.0.2.10" BINARY="":
    ./scripts/qemu/launch-mps2-an385.sh --tap {{TAP}} --ip {{IP}} --binary {{BINARY}}

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
    rm -rf packages/reference/qemu-smoltcp-bridge/target
    echo "QEMU bare-metal build artifacts cleaned"

# =============================================================================
# Run examples (build + launch QEMU MPS2-AN385 with slirp networking)
# =============================================================================

# Run bare-metal talker on QEMU
talker: build
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel examples/qemu-arm-baremetal/rust/zenoh/talker/target/thumbv7m-none-eabi/release/qemu-bsp-talker \
        -nic user,model=lan9118

# Run bare-metal listener on QEMU
listener: build
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel examples/qemu-arm-baremetal/rust/zenoh/listener/target/thumbv7m-none-eabi/release/qemu-bsp-listener \
        -nic user,model=lan9118

# Start zenohd router for bare-metal QEMU examples (port 7450)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7450 --no-multicast-scouting
