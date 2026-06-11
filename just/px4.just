set working-directory := '..'

default:
    @just --list --justfile {{ source_file() }}

# Initialise PX4 submodules (shallow). Includes:
#   third-party/px4/px4-rs           — Rust framework for PX4 modules
#   third-party/px4/PX4-Autopilot    — PX4 firmware tree
# Phase 203 — PX4-Autopilot's pin lags `main` and isn't advertised, so a plain
# `--depth 1` submodule update can't reach it (empty checkout, no `Makefile`).
# nros ≥ 0.3.7 falls back to a depth-1 fetch-by-SHA, so the TOP tree is a real
# shallow snapshot (Makefile present). PX4's ~50 OWN sub-submodules (cmake,
# mavlink, ecl, …) are pulled shallow below. Total ~500 MB.
# Required for `just px4 build-sitl` and `just px4 test-sitl`.
[group("setup")]
setup:
    # Phase 197.3 — px4-rs + PX4-Autopilot are index sources ([source.px4-rs],
    # [source.px4-autopilot]); provision the nano-ros submodules via nros (the
    # SSOT for refs) instead of an inline `git submodule update`.
    nros setup --source px4-rs --source px4-autopilot
    # PX4-Autopilot has ~50 of its OWN sub-submodules (cmake, mavlink, ecl, …) —
    # that recursion is PX4's concern, not nano-ros source provisioning.
    git -C third-party/px4/PX4-Autopilot submodule update --init \
        --depth 1 --recommend-shallow --recursive
    # Install PX4's Python build deps user-local so `just px4 build-sitl`
    # doesn't fail mid-build on missing imports (kconfiglib for the
    # menuconfig step, pyros-genmsg + jinja2 for uORB topic-header
    # generation, …). Mirrors PX4's own `Tools/setup/ubuntu.sh` step.
    pip3 install --user -r third-party/px4/PX4-Autopilot/Tools/setup/requirements.txt

# Diagnose PX4 install status (read-only).
[group("setup")]
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
[group("main")]
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
[group("examples")]
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
[group("examples")]
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
[group("examples")]
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
[group("main")]
build-examples: build-sitl build-sitl-cpp
    @echo "PX4 examples built."

# Build PX4 test fixtures.
#
# Phase 233.4 — the XRCE companion host test (`nros-tests::px4_xrce`) runs the
# prebuilt `px4-stub` example. Generate its `px4_msgs` bindings from the PX4
# `.msg` tree (no `nros ws sync` path — px4_msgs isn't an ament package) then
# build the standalone example to `target-xrce/` where `build_example_rmw`
# resolves it. Companion gets the same bindings so a `just px4 build-examples`
# copy-out builds too.
[group("full-matrix")]
build-fixtures:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    PX4_DIR="${PX4_AUTOPILOT_DIR:-$(pwd)/third-party/px4/PX4-Autopilot}"
    if [ ! -d "$PX4_DIR/msg" ]; then
        echo "[PREREQ] PX4-Autopilot .msg tree not found at $PX4_DIR/msg — run: just px4 setup"
        exit 0
    fi
    nros_cli="$(nros_cli_bin)"
    if ! "$nros_cli" generate-px4-msgs --help >/dev/null 2>&1; then
        echo "[PREREQ] '$nros_cli' lacks 'generate-px4-msgs' — rebuild the in-tree CLI (just setup-cli)"
        exit 0
    fi
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    for ex in px4-stub offboard-companion px4-probe; do
        dir="examples/px4/rust/xrce/$ex"
        echo "  → generate px4_msgs for $dir"
        "$nros_cli" generate-px4-msgs --px4 "$PX4_DIR" --output "$dir/generated" >/dev/null
    done
    for ex in px4-stub offboard-companion px4-probe; do
        echo "  → cargo build $ex (rmw-xrce, target-xrce/)"
        ( cd "examples/px4/rust/xrce/$ex" \
          && cargo build $cargo_profile_args --no-default-features --features rmw-xrce --target-dir target-xrce )
    done
    echo "PX4 XRCE fixtures built."

# Build every PX4 tier: core crates, example integrations, and fixtures.
[group("full-matrix")]
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
[group("main")]
test:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    cargo test $cargo_profile_args -p nros-rmw-uorb --features std,test-helpers --lib --tests
    cargo test $cargo_profile_args -p nros-px4 --features test-helpers --tests

# Run E2E test against PX4 SITL. Builds SITL first, then runs the test.
[group("debug")]
test-sitl: build-sitl
    #!/usr/bin/env bash
    # Phase 208.D.3 — `px4_e2e` moved to the out-of-workspace
    # `nros-px4-sitl-test` crate so the root workspace doesn't try to
    # resolve `px4-rs` on a fresh clone. Run it from its own dir.
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    cd packages/testing/nros-px4-sitl-test
    cargo nextest run "${cargo_nextest_args[@]}" --test px4_e2e

# Combined CI: check + unit tests (SITL gated separately via test-sitl).
[group("full-matrix")]
ci: check test build-targets

# Clean PX4 example build artifacts without touching downloaded submodules.
[group("main")]
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
