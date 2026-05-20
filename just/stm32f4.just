set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Build STM32F4 core artifacts.
build:
    @echo "STM32F4 has no separate core build; example coverage lives in fixtures today."

# Build STM32F4 examples.
build-examples:
    @echo "STM32F4 examples are currently covered by 'just stm32f4 build-fixtures'."

# Build all example binaries used as test fixtures (STM32F4 bare-metal Rust).
# Skips cleanly if the ARM cross-toolchain is missing.
build-fixtures:
    #!/usr/bin/env bash
    set -e
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "STM32F4 skip: arm-none-eabi-gcc not found"; exit 0
    fi
    echo "Building STM32F4 test fixtures..."
    # Phase 160 follow-up — parallel cargo builds.
    rust_dirs=()
    for ex in talker talker-rtic listener-rtic service-server-rtic \
              service-client-rtic action-server-rtic action-client-rtic; do
        dir="examples/stm32f4/rust/zenoh/$ex"
        if [ ! -d "$dir" ]; then
            echo "ERROR: $dir missing"
            exit 1
        fi
        rust_dirs+=("$dir")
    done
    printf '%s\n' "${rust_dirs[@]}" | parallel --jobs ${NROS_BUILD_JOBS:-8} --halt now,fail=1 --line-buffer \
        'echo "  → {}" && cd {} && cargo build --release --target thumbv7em-none-eabihf --quiet'

# Build every STM32F4 tier: core artifacts, examples, and test fixtures.
build-all: build build-examples build-fixtures
    @echo "STM32F4 build-all completed."
