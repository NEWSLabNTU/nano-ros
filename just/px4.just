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
    # PX4's uORB topic-header generator + Kconfig step pull in Python
    # modules listed in `Tools/setup/requirements.txt`. The few that
    # are load-bearing for `build-sitl{,-cpp}` (and which silently
    # break the build with "No module named X" if they're missing
    # after a submodule update) get spot-checked here so doctor
    # surfaces the staleness before the user spends 30s waiting for
    # ninja to fail at step [2/1140].
    for mod in genmsg kconfiglib jinja2 em; do
        if python3 -c "import $mod" >/dev/null 2>&1; then
            echo "  [OK] python3: $mod"
        else
            echo "  [MISSING] python3: $mod — run: just px4 setup"
            fail=1
        fi
    done
    if [ -n "${PX4_AUTOPILOT_DIR:-}" ]; then
        echo "  [INFO] PX4_AUTOPILOT_DIR set to $PX4_AUTOPILOT_DIR (overrides submodule)"
    fi
    exit $fail

# Build the px4 RMW + board crates (host-mock std mode).
build:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo build $cargo_profile_args -p nros-rmw-uorb -p nros-px4 --features std

# Cross-build nros-rmw-uorb for every PX4 target triple in
# `cmake/px4-rust.cmake`. Pre-flight check that Phase 90.2b's
# no_std + alloc registry path actually links on the platforms
# real PX4 modules deploy to: STM32F1/F4 (M3), Pixhawk 4 (M4F),
# STM32H7 / NXP RT117x (M7).
build-targets:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    echo "→ thumbv7m-none-eabi (Cortex-M3)"
    cargo build $cargo_profile_args -p nros-rmw-uorb \
        --target thumbv7m-none-eabi --no-default-features --features alloc
    echo "→ thumbv7em-none-eabihf (Cortex-M4F / Pixhawk 4)"
    cargo build $cargo_profile_args -p nros-rmw-uorb \
        --target thumbv7em-none-eabihf --no-default-features --features alloc
    echo "→ thumbv8m.main-none-eabihf (Cortex-M7 / STM32H7)"
    cargo build $cargo_profile_args -p nros-rmw-uorb \
        --target thumbv8m.main-none-eabihf --no-default-features --features alloc

# Build PX4 SITL with the nano-ros uORB C++ register-check module
# linked in via EXTERNAL_MODULES_LOCATION. This is the Phase 115.K.4.5
# validation gate: it compiles the entire `nros-rmw-uorb-cpp` library
# inline inside a real `px4_add_module()` build context, including the
# `SubscriptionCallbackWorkItem` push-wake glue that requires
# `<px4_boardconfig.h>` (per-board Kconfig output, only exists during
# a SITL build).
#
# Cold build ~10 min; warm <2 s. Requires `just px4 setup`.
build-sitl-cpp:
    #!/usr/bin/env bash
    set -e
    PX4_DIR="${PX4_AUTOPILOT_DIR:-$(pwd)/third-party/px4/PX4-Autopilot}"
    EXT_DIR="$(pwd)/examples/px4/cpp/uorb"
    if [ ! -d "$PX4_DIR/Tools" ]; then
        echo "ERROR: PX4-Autopilot tree not found at $PX4_DIR"
        echo "Run: just px4 setup"
        exit 1
    fi
    echo "Building PX4 SITL with EXTERNAL_MODULES_LOCATION=$EXT_DIR"
    make -C "$PX4_DIR" px4_sitl_default \
        EXTERNAL_MODULES_LOCATION="$EXT_DIR"

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

# Build PX4 examples.
build-examples: build-sitl build-sitl-cpp
    @echo "PX4 examples built."

# Build PX4 test fixtures.
build-fixtures:
    @echo "PX4 has no separate test fixture build today."

# Build every PX4 tier: core crates, example integrations, and fixtures.
build-all: build build-targets build-examples build-fixtures
    @echo "PX4 build-all completed."

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
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo test $cargo_profile_args -p nros-rmw-uorb --features std,test-helpers --lib --tests
    cargo test $cargo_profile_args -p nros-px4 --features test-helpers --tests

# Run E2E test against PX4 SITL. Builds SITL first, then runs the test.
test-sitl: build-sitl
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    nextest_profile_args=($(nros_nextest_profile_args))
    cargo nextest run "${nextest_profile_args[@]}" -p nros-tests --features px4-sitl --test px4_e2e

# Combined CI: check + unit tests (SITL gated separately via test-sitl).
ci: check test build-targets

# Clean PX4 example build artifacts without touching downloaded submodules.
clean:
    #!/usr/bin/env bash
    set -e
    PX4_DIR="${PX4_AUTOPILOT_DIR:-$(pwd)/third-party/px4/PX4-Autopilot}"
    rm -rf packages/rmw/nros-rmw-uorb/target packages/platforms/nros-px4/target
    if [ -d examples/px4 ]; then
        find examples/px4 -type d -name target -exec rm -rf {} + 2>/dev/null || true
    fi
    if [ -d "$PX4_DIR/build" ]; then
        rm -rf "$PX4_DIR"/build/px4_sitl_default "$PX4_DIR"/build/px4_fmu-v6x_default
    fi
    echo "PX4 build artifacts cleaned"
