set working-directory := '..'

NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

default:
    @just --list --justfile {{source_file()}}

# Install ESP32 prerequisites: Espressif QEMU fork (required by every
# networked ESP32 test — the stock Debian/Ubuntu `qemu-system-riscv32`
# doesn't know the `esp32c3` machine model). Post-84.F4 zenoh-pico
# itself is cross-compiled by `zpico-sys/build.rs` during the
# example's cargo build; no standalone library build step is needed
# or wired up anywhere in the project.
#
# Phase 213.A.4 — this recipe provisions the `[board.qemu-esp32-baremetal]`
# board (Espressif QEMU fork, esp32c3 machine). It has `packages = []` in
# the index — the riscv32 target comes from `just workspace rust-targets`
# (rustup), and the QEMU fork is the source-built `[tool.esp32-qemu]`. There
# is no `just qemu-esp32-baremetal setup` sibling; this recipe is the entry point.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    # Phase 197.4/196.9 — provision the build sources (zenoh-pico/mbedtls via the
    # default zenoh RMW; the riscv32 target is rustup-managed) so the cross build
    # resolves. The esp32 board has no toolchain package (build-std), so this is
    # the source set.
    source scripts/build/cargo.sh
    "$(nros_cli_bin)" setup qemu-esp32-baremetal
    # Probe for the `esp32c3` machine model, not the binary's mere existence —
    # the ESP32 e2e TESTS need Espressif's QEMU fork (stock qemu-system-riscv32
    # only knows `virt`). Build does NOT need it, so provision best-effort: the
    # fork's source-build wants qemu deps that aren't always present, and that
    # must not block the build phase (the e2e step gates on its presence).
    if command -v qemu-system-riscv32 &>/dev/null \
            && qemu-system-riscv32 -machine help 2>/dev/null | grep -q '\besp32c3\b'; then
        echo "Espressif qemu-system-riscv32 already installed: $(qemu-system-riscv32 --version | head -1)"
    else
        # Phase 197.3 — the Espressif QEMU fork is the source-built
        # [tool.esp32-qemu] in nros-sdk-index.toml; provision it via nros.
        nros setup --tool esp32-qemu || echo "esp32-qemu unavailable (e2e will skip); build unaffected"
    fi

# Diagnose ESP32 install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if command -v qemu-system-riscv32 &>/dev/null \
            && qemu-system-riscv32 -machine help 2>/dev/null | grep -q '\besp32c3\b'; then
        echo "  [OK] qemu-system-riscv32 (Espressif fork, esp32c3 machine)"
    else
        echo "  [MISSING] Espressif qemu-system-riscv32 with esp32c3 machine (run: just esp32 setup)"
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

# Build ESP32 core artifacts. ESP32 has no separate core build step
# (the zenoh-pico cross-build runs inside each example's cargo build,
# Phase 84.F4) — `build` is an alias for `build-examples` so the standard
# `just <plat> build` UX returns the same artifact set as every other
# platform. Phase 208.D.5.
[group("main")]
build: build-examples

# Build ESP32 examples (requires the workspace nightly — ESP32-C3 / riscv32imc).
[group("main")]
build-examples:
    #!/usr/bin/env bash
    set -e
    # Phase 244.D2 — the WiFi-only plain esp32 fixtures (talker/listener) were
    # dropped (WiFi un-emulable). The only `platform = "esp32"` fixture left is the
    # ESP32-C3 QEMU (OpenETH) workspace Entry, built by the workspace lane below;
    # `fixtures-build.sh esp32 rust` would now select zero plain fixtures.
    echo "Building ESP32 examples (from examples/fixtures.toml)..."
    # Phase 225.O — ESP32-C3 QEMU workspace Entry (Node + Bringup + Entry
    # pkgs) through the documented workspace workflow (`nros ws sync`,
    # `nros codegen-system`, then cargo). Mirrors the freertos /
    # threadx_linux build-examples workspace-lane tail. The per-row `env`
    # in examples/fixtures.toml carries RUSTUP_TOOLCHAIN +
    # CARGO_UNSTABLE_BUILD_STD, so this call needs no extra toolchain env.
    bash scripts/build/workspace-fixtures-build.sh esp32 rust
    echo "ESP32 examples built!"

# Build ESP32 QEMU examples (requires nightly)
[group("examples")]
build-qemu:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    target_profile_dir="$(nros_cargo_target_profile_dir)"
    echo "Building ESP32 QEMU fixtures (from examples/fixtures.toml)..."
    for id in qemu-esp32-baremetal-talker qemu-esp32-baremetal-listener; do
        RUSTUP_TOOLCHAIN="{{NIGHTLY}}" SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" \
            bash scripts/build/fixtures-build.sh qemu-esp32-baremetal rust --id "$id"
    done
    echo ""
    echo "Creating flash images..."
    mkdir -p build/esp32-qemu
    for ex in talker listener; do
        bin_name="esp32-qemu-$ex"
        elf="examples/qemu-esp32-baremetal/rust/$ex/target/riscv32imc-unknown-none-elf/$target_profile_dir/$bin_name"
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

# Phase 88.15.f — build the ESP32-C3 nros-log smoke fixture +
# espflash-pack the flash image alongside the ELF so the harness
# can pick it up via `build_logging_smoke_esp32_qemu_flash`.
[group("examples")]
build-logging-smoke:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    target_profile_dir="$(nros_cargo_target_profile_dir)"
    echo "Building logging-smoke-esp32-qemu..."
    RUSTUP_TOOLCHAIN="{{NIGHTLY}}" SSID="${SSID:-test}" PASSWORD="${PASSWORD:-test}" \
        bash scripts/build/fixtures-build.sh qemu-esp32-baremetal rust --id qemu-esp32-baremetal-logging-smoke
    echo "Creating logging-smoke-esp32-qemu flash image..."
    fixture=packages/testing/nros-tests/bins/logging-smoke-esp32-qemu
    elf="$fixture/target/riscv32imc-unknown-none-elf/$target_profile_dir/logging-smoke-esp32-qemu"
    bin="$elf.bin"
    if command -v espflash &>/dev/null; then
        espflash save-image --chip esp32c3 --flash-size 4mb --merge "$elf" "$bin"
        echo "  $bin"
    else
        echo "  WARNING: espflash not found, skipping flash image"
        echo "  Install with: cargo install espflash"
    fi

# Build ESP32 test fixtures.
[group("full-matrix")]
build-fixtures: build-qemu build-logging-smoke
    #!/usr/bin/env bash
    set -e
    # The ESP32-C3 QEMU workspace Entry (`esp32_entry`) that
    # `test_esp32_workspace_entry_e2e` consumes via
    # `get_prebuilt_esp32_qemu_workspace_entry`. Previously only `build-examples`
    # built it, but the test's skip message (correctly) points users to
    # `build-fixtures` — so this lane must stage it too. Idempotent + cached.
    bash scripts/build/workspace-fixtures-build.sh esp32 rust
    # esp-idf build-stage fixtures (issue 0041): build the idf.py ELF the
    # cli_bringup_esp_idf test consumes, so it doesn't run idf.py at test time.
    # Self-gates on idf.py/IDF env; non-fatal (a failed build leaves no stamp,
    # and the test skips per tier). Slow (~7 min/ELF), so scoped to this esp
    # lane, not build-test-fixtures.
    bash scripts/build/idf-fixtures.sh || true
    echo "ESP32 fixtures built."

# Build every ESP32 tier: core artifacts, examples, and test fixtures.
[group("full-matrix")]
build-all: build build-examples build-fixtures
    @echo "ESP32 build-all completed."

# Run basic QEMU ESP32-C3 boot test
[group("debug")]
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
#
# Phase 214.Q: `build-fixtures` is sequenced as a prereq so the nextest
# harness never explodes with "fixture not built" errors. The harness
# fails loud on missing fixtures by design (Phase 181) — recipes own the
# build-then-test sequencing.
[group("main")]
test verbose="": build-fixtures
    # Delegate to the shared skip-tolerant runner: the esp32_emulator tests
    # `skip!` when the Espressif QEMU fork / espflash / riscv32 target are absent
    # (best-effort emulator), and _nextest-platform treats an all-[SKIPPED] run
    # as pass while still failing on real errors.
    @just _nextest-platform esp32_emulator {{verbose}}

# Run all ESP32 tests (same as test)
[group("full-matrix")]
test-all verbose="": (test verbose)

# CI: doctor + test
[group("full-matrix")]
ci: doctor test

# Clean ESP32 build artifacts
[group("main")]
clean:
    #!/usr/bin/env bash
    for ex in talker listener; do
        rm -rf "examples/qemu-esp32-baremetal/rust/$ex/target"
    done
    rm -rf build/esp32-qemu build/esp32-zenoh-pico
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
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    exec "$(nros_zenohd_bin)" --listen tcp/127.0.0.1:7454 --no-multicast-scouting
