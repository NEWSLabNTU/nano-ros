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
# Required for `just px4 build-sitl-cpp` and `just px4 test-sitl`.
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
    # Install PX4's Python build deps user-local so `just px4 build-sitl-cpp`
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
    # are load-bearing for `build-sitl-cpp` (and which silently
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
    EXT_DIR="$(pwd)/packages/testing/nros-px4-register-check"
    if [ ! -d "$PX4_DIR/Tools" ]; then
        echo "ERROR: PX4-Autopilot tree not found at $PX4_DIR"
        echo "Run: just px4 setup"
        exit 1
    fi
    echo "Building PX4 SITL with EXTERNAL_MODULES_LOCATION=$EXT_DIR"
    make -C "$PX4_DIR" px4_sitl_default \
        EXTERNAL_MODULES_LOCATION="$EXT_DIR"

# Build PX4 examples. (phase-277 W7: the former `build-sitl` recipe pointed
# EXTERNAL_MODULES_LOCATION at the README-only `examples/px4/rust/uorb/`
# placeholder — it compiled no nano-ros module and the placeholder dir is
# retired; `build-sitl-cpp` is the real EXTERNAL_MODULES_LOCATION build.)
[group("main")]
build-examples: build-sitl-cpp
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
        dir="examples/px4/rust/companion/$ex"
        echo "  → generate px4_msgs for $dir"
        "$nros_cli" generate-px4-msgs --px4 "$PX4_DIR" --output "$dir/generated" >/dev/null
    done
    for ex in px4-stub offboard-companion px4-probe; do
        echo "  → cargo build $ex (rmw-xrce, target-xrce/)"
        ( cd "examples/px4/rust/companion/$ex" \
          && cargo build $cargo_profile_args --no-default-features --features rmw-xrce --target-dir target-xrce )
    done
    echo "PX4 XRCE fixtures built."

# Build every PX4 tier: example integrations and fixtures.
#
# phase-321 W1.c — `build` and `build-targets` are gone from this list because
# they built `nros-rmw-uorb` and `nros-px4` as CARGO crates, and both were
# deleted in phase-115.K.4. The surviving uORB backend is the C++ CMake project
# at `packages/px4/nros-rmw-uorb`, which `build-examples` covers.
[group("full-matrix")]
build-all: build-examples build-fixtures
    @echo "PX4 build-all completed."

# Run E2E tests against PX4 SITL. Builds SITL + the XRCE fixtures first.
#
#   px4_e2e       — Track A (uORB in-firmware modules)
#   px4_xrce_e2e  — Track B (real PX4 uxrce_dds_client → agent → nano-ros
#                   companion receives /fmu/out telemetry over nros-rmw-xrce)
[group("debug")]
test-sitl: build-sitl-cpp build-fixtures
    #!/usr/bin/env bash
    # Phase 208.D.3 — `px4_e2e` moved to the out-of-workspace
    # `nros-px4-sitl-test` crate so the root workspace doesn't try to
    # resolve `px4-rs` on a fresh clone. Run it from its own dir.
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    cd packages/testing/nros-px4-sitl-test
    cargo nextest run "${cargo_nextest_args[@]}" --test px4_e2e --test px4_xrce_e2e

# Combined CI: build the C++ uORB integration (SITL gated separately via
# test-sitl).
#
# phase-321 W1.c — this was `ci: check test build-targets`, all three of which
# built the CARGO crates `nros-rmw-uorb` / `nros-px4` that phase-115.K.4
# deleted. Only archived phase docs referenced this recipe, so it had been
# unrunnable and unnoticed. What survives is the C++ CMake project, which
# `build-examples` compiles inside a real `px4_add_module()` context.
[group("full-matrix")]
ci: build-examples

# Clean PX4 example build artifacts without touching downloaded submodules.
[group("main")]
clean:
    #!/usr/bin/env bash
    set -e
    PX4_DIR="${PX4_AUTOPILOT_DIR:-$(pwd)/third-party/px4/PX4-Autopilot}"
    # (phase-321 W1.c — the old `rm -rf packages/rmw/nros-rmw-uorb/target
    # packages/platforms/nros-px4/target` line is gone: BOTH crates were deleted
    # in phase-115.K.4 and `packages/rmw/` has never existed in this tree.)
    if [ -d examples/px4 ]; then
        find examples/px4 -type d -name target -exec rm -rf {} + 2>/dev/null || true
    fi
    if [ -d "$PX4_DIR/build" ]; then
        rm -rf "$PX4_DIR"/build/px4_sitl_default "$PX4_DIR"/build/px4_fmu-v6x_default
    fi
    echo "PX4 build artifacts cleaned"
