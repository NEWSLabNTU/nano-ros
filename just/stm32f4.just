set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install STM32F4 prerequisites (arm-none-eabi-gcc + openocd) via `nros setup`.
# Phase 213.A.2 — thin caller of `nros setup stm32f4`, matching the pattern
# every other platform setup recipe uses. Real hardware uses openocd to flash;
# the recipe does NOT pull a qemu fork (stm32f4 is not a QEMU board).
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    "$(nros_cli_bin)" setup stm32f4 --rmw zenoh

# Build STM32F4 core artifacts.
[group("main")]
build:
    @echo "STM32F4 has no separate core build; example coverage lives in fixtures today."

# Build STM32F4 examples.
[group("main")]
build-examples:
    @echo "STM32F4 examples are currently covered by 'just stm32f4 build-fixtures'."

# Build all example binaries used as test fixtures (STM32F4 bare-metal Rust).
# Skips cleanly if the ARM cross-toolchain is missing.
[group("full-matrix")]
build-fixtures:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    export cargo_profile_args
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "STM32F4 skip: arm-none-eabi-gcc not found"; exit 0
    fi
    echo "Building STM32F4 test fixtures..."
    bash scripts/build/fixtures-build.sh stm32f4 rust

# Build every STM32F4 tier: core artifacts, examples, and test fixtures.
[group("full-matrix")]
build-all: build build-examples build-fixtures
    @echo "STM32F4 build-all completed."

# Clean STM32F4 example and fixture build artifacts.
[group("main")]
clean:
    #!/usr/bin/env bash
    for ex in talker talker-rtic talker-embassy listener-rtic service-server-rtic \
              service-client-rtic action-server-rtic action-client-rtic; do
        rm -rf "examples/stm32f4/rust/$ex/target"
    done
    # Phase 226.D — shared fixture-only Cargo target dir.
    rm -rf build/fixtures-cargo/stm32f4
    echo "STM32F4 build artifacts cleaned"
