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
    cargo_frontends="$(nros_cargo_frontend_jobs)"
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "STM32F4 skip: arm-none-eabi-gcc not found"; exit 0
    fi
    echo "Building STM32F4 test fixtures..."
    # Phase 160 follow-up — parallel cargo builds.
    rust_dirs=()
    for ex in talker talker-rtic listener-rtic service-server-rtic \
              service-client-rtic action-server-rtic action-client-rtic; do
        dir="examples/stm32f4/rust/$ex"
        if [ ! -d "$dir" ]; then
            echo "ERROR: $dir missing"
            exit 1
        fi
        rust_dirs+=("$dir")
    done
    if [ "${NROS_JOBSERVER:-}" = "1" ]; then
        for dir in "${rust_dirs[@]}"; do
            echo "  → $dir"
            ( cd "$dir" && cargo build $cargo_profile_args --target thumbv7em-none-eabihf --quiet )
        done
    else
        printf '%s\n' "${rust_dirs[@]}" | parallel --jobs "$cargo_frontends" --halt now,fail=1 --line-buffer \
            'echo "  → {}" && cd {} && cargo build $cargo_profile_args --target thumbv7em-none-eabihf --quiet'
    fi

# Build every STM32F4 tier: core artifacts, examples, and test fixtures.
[group("full-matrix")]
build-all: build build-examples build-fixtures
    @echo "STM32F4 build-all completed."

# Clean STM32F4 example and fixture build artifacts.
[group("main")]
clean:
    #!/usr/bin/env bash
    for ex in talker talker-rtic listener-rtic service-server-rtic \
              service-client-rtic action-server-rtic action-client-rtic; do
        rm -rf "examples/stm32f4/rust/$ex/target"
    done
    echo "STM32F4 build artifacts cleaned"
