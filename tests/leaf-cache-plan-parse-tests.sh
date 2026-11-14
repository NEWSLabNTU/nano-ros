#!/usr/bin/env bash
# `nros_plan_cargo_build_args` reads the cargo build line of an
# `nros build --dry-run` plan, whatever env prefix `Handoff::display()` puts
# before it. The cases are lines the nightly printed (2026-11-14) plus the
# shapes that must NOT be read as a cargo build.
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/build/leaf-cache.sh
source scripts/build/leaf-cache.sh

fail=0
check() { # <name> <plan> <expected>
    local got
    got="$(printf '%s\n' "$2" | nros_plan_cargo_build_args)"
    if [ "$got" = "$3" ]; then
        echo "  ok   $1"
    else
        echo "  FAIL $1"; echo "       want: $3"; echo "       got:  $got"; fail=1
    fi
}

check "bare cargo build" \
    "cargo build --manifest-path build/x/Cargo.toml --config build/x/nros-cargo.toml --frozen" \
    "manifest build/x/Cargo.toml
config build/x/nros-cargo.toml"

check "env prefix with a space in a value, and a key=value --config after the file" \
    'nros build: pubsub_bringup:t -> board rtic-mps2-an385 (platform bare-metal), driver cargo
CARGO_TARGET_THUMBV7M_NONE_EABI_RUSTFLAGS=-Csave-temps -Clink-arg=-Map=/w/image.map CFLAGS_thumbv7m_none_eabi=-fstack-usage -fcallgraph-info=su RUSTC_WRAPPER= cargo build --manifest-path build/bare-metal-zenoh/t_entry/Cargo.toml --config build/bare-metal-zenoh/t_entry/nros-cargo.toml --frozen --config build.build-dir="/w/build" --profile nros-relwithdebinfo' \
    "manifest build/bare-metal-zenoh/t_entry/Cargo.toml
config build/bare-metal-zenoh/t_entry/nros-cargo.toml"

check "RUSTUP_TOOLCHAIN prefix and an absolute cargo" \
    "RUSTUP_TOOLCHAIN=nightly-2026-04-11 /usr/local/cargo/bin/cargo build --manifest-path src/e/Cargo.toml --frozen" \
    "manifest src/e/Cargo.toml"

check "cmake plan: no cargo build line" \
    "cmake -S . -B build/x -G Ninja
cmake --build build/x" \
    ""

check "a message that only mentions cargo build is not a plan line" \
    "note: cargo build was not run" \
    ""

if [ "$fail" -ne 0 ]; then
    echo "leaf-cache-plan-parse: FAILED" >&2
    exit 1
fi
echo "leaf-cache-plan-parse: OK (5 cases)"
