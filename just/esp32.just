set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ESP32 prerequisites: Espressif QEMU + RISC-V zenoh-pico static lib.
setup:
    #!/usr/bin/env bash
    set -e
    if command -v qemu-system-riscv32 &>/dev/null; then
        echo "qemu-system-riscv32 already installed: $(qemu-system-riscv32 --version | head -1)"
    else
        ./scripts/esp32/install-espressif-qemu.sh
    fi
    if [ -f build/esp32-zenoh-pico/libzenohpico.a ]; then
        echo "zenoh-pico RISC-V already built: build/esp32-zenoh-pico/libzenohpico.a"
    else
        ./scripts/esp32/build-zenoh-pico.sh
    fi

# Diagnose ESP32 install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if command -v qemu-system-riscv32 &>/dev/null; then
        echo "  [OK] qemu-system-riscv32"
    else
        echo "  [MISSING] qemu-system-riscv32 (run: just esp32 setup)"
        fail=1
    fi
    if [ -f build/esp32-zenoh-pico/libzenohpico.a ]; then
        echo "  [OK] zenoh-pico riscv: build/esp32-zenoh-pico/libzenohpico.a"
    else
        echo "  [MISSING] zenoh-pico riscv (run: just esp32 setup)"
        fail=1
    fi
    if rustup target list --installed 2>/dev/null | grep -q '^riscv32imc-unknown-none-elf$'; then
        echo "  [OK] riscv32imc-unknown-none-elf target"
    else
        echo "  [MISSING] riscv32imc-unknown-none-elf target (run: just workspace rust-targets)"
        fail=1
    fi
    if command -v espflash &>/dev/null; then
        echo "  [OK] espflash"
    else
        echo "  [MISSING] espflash (run: just workspace cargo-tools)"
        fail=1
    fi
    exit $fail

# Build ESP32 examples (requires nightly)
build:
    #!/usr/bin/env bash
    set -e
    echo "Building ESP32 examples..."
    for ex in talker listener; do
        echo "  Building esp32/rust/zenoh/$ex..."
        (cd examples/esp32/rust/zenoh/$ex && \
         SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" cargo +nightly build --release)
    done
    echo "ESP32 examples built!"

# Build ESP32 QEMU examples (requires nightly)
build-qemu:
    #!/usr/bin/env bash
    set -e
    echo "Building ESP32 QEMU examples..."
    for ex in talker listener; do
        echo "  Building qemu-esp32/rust/zenoh/$ex..."
        (cd examples/qemu-esp32-baremetal/rust/zenoh/$ex && cargo +nightly build --release)
    done
    echo ""
    echo "Creating flash images..."
    mkdir -p build/esp32-qemu
    for ex in talker listener; do
        bin_name="esp32-qemu-$ex"
        elf="examples/qemu-esp32-baremetal/rust/zenoh/$ex/target/riscv32imc-unknown-none-elf/release/$bin_name"
        out="build/esp32-qemu/$bin_name.bin"
        if command -v espflash &>/dev/null; then
            espflash save-image --chip esp32c3 --flash-size 4mb --merge "$elf" "$out"
            echo "  $out"
        else
            echo "  WARNING: espflash not found, skipping flash image for $ex"
            echo "  Install with: cargo install espflash"
        fi
    done
    echo "ESP32 QEMU examples built!"

# Run basic QEMU ESP32-C3 boot test
test-basic: build-qemu
    #!/usr/bin/env bash
    echo "ESP32-C3 QEMU boot test"
    echo "========================"
    echo ""
    if ! command -v qemu-system-riscv32 &>/dev/null; then
        echo "WARNING: qemu-system-riscv32 not found - skipping runtime test"
        echo "Flash images are at: build/esp32-qemu/"
        exit 0
    fi
    echo "Running boot test..."
    tmpfile=$(mktemp)
    trap 'rm -f "$tmpfile"' EXIT
    timeout 20 qemu-system-riscv32 -M esp32c3 -icount 3 -nographic \
        -drive "file=build/esp32-qemu/esp32-qemu-talker.bin,if=mtd,format=raw" \
        -nic none \
        > "$tmpfile" 2>&1 || true
    cat "$tmpfile"
    echo ""
    if grep -q "nros ESP32-C3 QEMU BSP" "$tmpfile"; then
        echo "[PASS] ESP32-C3 QEMU boot test - BSP initialized"
    else
        echo "[FAIL] ESP32-C3 QEMU boot test - BSP banner not found"
        exit 1
    fi

# Run ESP32-C3 QEMU integration tests (build, boot, E2E via nextest)
test verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test esp32_emulator --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run all ESP32 tests (same as test)
test-all verbose="": (test verbose)

# CI: doctor + test
ci: doctor test

# Build zenoh-pico for ESP32-C3 (RISC-V RV32IMC)
build-zenoh-pico:
    @./scripts/esp32/build-zenoh-pico.sh

# Clean zenoh-pico RISC-V build
clean-zenoh-pico:
    ./scripts/esp32/build-zenoh-pico.sh --clean

# Clean ESP32 build artifacts
clean: clean-zenoh-pico
    #!/usr/bin/env bash
    for ex in talker listener; do
        rm -rf "examples/esp32/rust/zenoh/$ex/target"
        rm -rf "examples/qemu-esp32-baremetal/rust/zenoh/$ex/target"
    done
    rm -rf build/esp32-qemu
    echo "ESP32 build artifacts cleaned"

# =============================================================================
# Run examples (build + launch ESP32-C3 QEMU)
# =============================================================================

# Run ESP32 talker on QEMU
talker: build-qemu
    qemu-system-riscv32 -M esp32c3 -icount 3 -nographic \
        -drive "file=build/esp32-qemu/esp32-qemu-talker.bin,if=mtd,format=raw" \
        -nic user,model=open_eth

# Run ESP32 listener on QEMU
listener: build-qemu
    qemu-system-riscv32 -M esp32c3 -icount 3 -nographic \
        -drive "file=build/esp32-qemu/esp32-qemu-listener.bin,if=mtd,format=raw" \
        -nic user,model=open_eth

# Start zenohd router for ESP32 QEMU examples (port 7454)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7454 --no-multicast-scouting
