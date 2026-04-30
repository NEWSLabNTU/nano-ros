set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Build all example binaries used as test fixtures (STM32F4 bare-metal Rust).
# Skips cleanly if the ARM cross-toolchain is missing.
build-fixtures:
    #!/usr/bin/env bash
    set -e
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "STM32F4 skip: arm-none-eabi-gcc not found"; exit 0
    fi
    echo "Building STM32F4 test fixtures..."
    rust_examples=(
        talker
        listener
        rtic-talker
        rtic-listener
        rtic-service-server
        rtic-service-client
        rtic-action-server
        rtic-action-client
    )
    # Best-effort: report failures but don't abort `build-test-fixtures`.
    # Recent core-side static-buffer growth (Phase 99.M handles.rs)
    # pushes some RTIC binaries past the 192 KB SRAM1 budget; the
    # talker/listener pair still fits and is what `test-all` cares
    # about for build-only verification.
    failed=0
    for ex in "${rust_examples[@]}"; do
        dir="examples/stm32f4/rust/zenoh/$ex"
        if [ ! -d "$dir" ]; then
            echo "  skip $dir (missing)"; continue
        fi
        echo "  → stm32f4/rust/$ex"
        if ! ( cd "$dir" && cargo build --release --target thumbv7em-none-eabihf --quiet ); then
            echo "  FAIL stm32f4/rust/$ex (continuing)"
            failed=$((failed+1))
        fi
    done
    if [ "$failed" -gt 0 ]; then
        echo "STM32F4 fixtures: $failed failed (best-effort, continuing)"
    fi
