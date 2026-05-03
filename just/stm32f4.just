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
        rtic-talker
        rtic-listener
        rtic-service-server
        rtic-service-client
        rtic-action-server
        rtic-action-client
    )
    # Strict: abort on the first per-example failure. Per-example RAM
    # tuning (e.g. `NROS_SUBSCRIPTION_BUFFER_SIZE` in the example's
    # `.cargo/config.toml`) is the right place to fix overflows now.
    for ex in "${rust_examples[@]}"; do
        dir="examples/stm32f4/rust/zenoh/$ex"
        if [ ! -d "$dir" ]; then
            echo "ERROR: $dir missing"
            exit 1
        fi
        echo "  → stm32f4/rust/$ex"
        ( cd "$dir" && cargo build --release --target thumbv7em-none-eabihf --quiet )
    done
