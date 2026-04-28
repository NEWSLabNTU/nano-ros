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
check:
    cargo fmt --check -p nros-rmw-uorb -p nros-px4
    cargo clippy -p nros-rmw-uorb -p nros-px4 --features std,test-helpers --all-targets -- -D warnings

# Run unit tests against the px4-rs std mock broker (no SITL required).
# Includes nros-px4's run_async/pump host-mock tests (90.5b acceptance).
#
# --test-threads=1: the px4-rs std mock spawns background HRT + WQ
# threads whose lifetimes outlast individual #[test] bodies. Running
# tests in parallel races those threads against each other across
# `_reset_broker()` boundaries, which manifests as "tcache_thread_shutdown:
# unaligned tcache chunk detected" at process exit. Serialising
# tests at the cargo level avoids the race.
test:
    cargo test -p nros-rmw-uorb --features std,test-helpers --lib --tests -- --test-threads=1
    cargo test -p nros-px4 --features test-helpers --tests -- --test-threads=1

# Run E2E test against PX4 SITL. Builds SITL first, then runs the test.
test-sitl: build-sitl
    cargo nextest run -p nros-tests --features px4-sitl --test px4_e2e

# Combined CI: check + unit tests (SITL gated separately via test-sitl).
ci: check test
