set working-directory := '..'

default:
    @just --list --justfile {{ source_file() }}

# Initialise PX4 submodules (shallow). Includes:
#   third-party/px4/px4-rs           — Rust framework for PX4 modules
#   third-party/px4/PX4-Autopilot    — PX4 firmware tree (recursive)
# PX4-Autopilot has ~50 own sub-submodules (cmake, mavlink, ecl, …).
# All cloned shallow; total ~500 MB. Required for `just px4 build-sitl`
# and `just px4 test-sitl`.
setup:
    git submodule update --init --depth 1 --recommend-shallow \
        third-party/px4/px4-rs \
        third-party/px4/PX4-Autopilot
    git -C third-party/px4/PX4-Autopilot submodule update --init \
        --depth 1 --recommend-shallow --recursive
    # Install PX4's Python build deps user-local so `just px4 build-sitl`
    # doesn't fail mid-build on missing imports (kconfiglib for the
    # menuconfig step, pyros-genmsg + jinja2 for uORB topic-header
    # generation, …). Mirrors PX4's own `Tools/setup/ubuntu.sh` step.
    pip3 install --user -r third-party/px4/PX4-Autopilot/Tools/setup/requirements.txt

# Diagnose PX4 install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "third-party/px4/px4-rs/crates" ]; then
        echo "  [OK] px4-rs submodule populated"
    else
        echo "  [MISSING] third-party/px4/px4-rs (run: just px4 setup)"
        fail=1
    fi
    if [ -d "third-party/px4/PX4-Autopilot/Tools" ]; then
        echo "  [OK] PX4-Autopilot submodule populated"
    else
        echo "  [MISSING] third-party/px4/PX4-Autopilot (run: just px4 setup)"
        fail=1
    fi
    for tool in cmake ninja gcc python3; do
        if command -v "$tool" >/dev/null 2>&1; then
            echo "  [OK] $tool"
        else
            echo "  [MISSING] $tool — install via your package manager"
            fail=1
        fi
    done
    if command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "  [OK] arm-none-eabi-gcc (target build available)"
    else
        echo "  [INFO] arm-none-eabi-gcc not installed — only SITL host build available"
    fi
    if [ -n "${PX4_AUTOPILOT_DIR:-}" ]; then
        echo "  [INFO] PX4_AUTOPILOT_DIR set to $PX4_AUTOPILOT_DIR (overrides submodule)"
    fi
    exit $fail

# Build the px4 RMW + board crates (host-mock std mode).
build:
    cargo build -p nros-rmw-uorb -p nros-px4 --features std

# Cross-build nros-rmw-uorb for every PX4 target triple in
# `cmake/px4-rust.cmake`. Pre-flight check that Phase 90.2b's
# no_std + alloc registry path actually links on the platforms
# real PX4 modules deploy to: STM32F1/F4 (M3), Pixhawk 4 (M4F),
# STM32H7 / NXP RT117x (M7).
build-targets:
    @echo "→ thumbv7m-none-eabi (Cortex-M3)"
    cargo build -p nros-rmw-uorb \
        --target thumbv7m-none-eabi --no-default-features --features alloc
    @echo "→ thumbv7em-none-eabihf (Cortex-M4F / Pixhawk 4)"
    cargo build -p nros-rmw-uorb \
        --target thumbv7em-none-eabihf --no-default-features --features alloc
    @echo "→ thumbv8m.main-none-eabihf (Cortex-M7 / STM32H7)"
    cargo build -p nros-rmw-uorb \
        --target thumbv8m.main-none-eabihf --no-default-features --features alloc

# Build PX4 SITL with the nano-ros uORB examples linked in via
# EXTERNAL_MODULES_LOCATION. Cold build ~10 min; warm <2 s (ninja
# "no work to do"). Requires `just px4 setup` to have populated
# PX4-Autopilot recursively.
build-sitl:
    #!/usr/bin/env bash
    set -e
    PX4_DIR="${PX4_AUTOPILOT_DIR:-$(pwd)/third-party/px4/PX4-Autopilot}"
    EXT_DIR="$(pwd)/examples/px4/rust/uorb"
    if [ ! -d "$PX4_DIR/Tools" ]; then
        echo "ERROR: PX4-Autopilot tree not found at $PX4_DIR"
        echo "Run: just px4 setup"
        exit 1
    fi
    echo "Building PX4 SITL with EXTERNAL_MODULES_LOCATION=$EXT_DIR"
    make -C "$PX4_DIR" px4_sitl_default \
        EXTERNAL_MODULES_LOCATION="$EXT_DIR"

# Check formatting + clippy on px4 crates.
#
# fmt uses `-p` per crate (not `--all`) because `cargo fmt --all`
# from the nano-ros workspace walks path-dep
# `third-party/px4/px4-rs/*` crates with nano-ros's stable rustfmt
# and rewrites them — drift against px4-rs's pinned nightly
# rustfmt. Pinning per-crate keeps fmt confined to nano-ros sources.
check:
    cargo fmt --check -p nros-rmw-uorb -p nros-px4
    cargo clippy -p nros-rmw-uorb -p nros-px4 --features std,test-helpers --all-targets -- -D warnings

# Run unit tests against the px4-rs std mock broker (no SITL required).
# Includes nros-px4's run_async/pump host-mock tests (90.5b acceptance).
#
# Default-parallel mode is fine after px4-rs upstream landed:
# - `3afaf2c2` HRT worker pool — fixes thread-spawn-per-call leak
# - HRT cancel evicts queue entries — fixes Sleep-Drop UAF
# - notify holds callback list lock + Vec<Arc<>> — fixes
#   Subscription-Drop UAF in callback dispatch
test:
    cargo test -p nros-rmw-uorb --features std,test-helpers --lib --tests
    cargo test -p nros-px4 --features test-helpers --tests

# Run E2E test against PX4 SITL. Builds SITL first, then runs the test.
test-sitl: build-sitl
    cargo nextest run -p nros-tests --features px4-sitl --test px4_e2e

# Combined CI: check + unit tests (SITL gated separately via test-sitl).
ci: check test build-targets
