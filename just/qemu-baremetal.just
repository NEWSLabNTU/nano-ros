set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
LOG_DIR := "test-logs"

# Project-local patched QEMU prefix (built by `just qemu setup-qemu`).
QEMU_PREFIX := absolute_path("build/qemu")
QEMU_BIN := QEMU_PREFIX / "bin/qemu-system-arm"

# Cargo target profile subdir (e.g. `nros-relwithdebinfo`, `release`, `debug`) —
# the fixture ELFs land under target/<triple>/<PROFILE_DIR>/. Must track the
# profile `build`/`build-fixtures` use, not a hardcoded `release` (the
# test-basic/wcet/lan9118 paths drifted from it once).
#
# phase-336: no literal fallback. A wrong-but-plausible default here sends every
# QEMU recipe at a path that does not exist and reports it as a missing fixture;
# the resolver failing loudly names the real problem (no CLI built yet).
#
# But it must not fail AT PARSE TIME. `just` evaluates a module's `:=` backticks
# when the justfile loads, for EVERY invocation — including `just setup-cli`,
# whose entire job is to build the CLI this query needs. A tree without one (a
# fresh clone, or a `~/.nros/bin/nros` older than `nros profile`) could not run
# ANY just recipe:
#
#     error: unrecognized subcommand 'profile'
#     error: backtick failed with exit code 1
#       ——▶ just/qemu-baremetal.just:19:16
#
# So the query yields EMPTY when the CLI cannot answer, and the recipes that
# consume it check for empty and fail with the actionable message. Loud at USE,
# silent at parse — which is what "fail loudly" wanted in the first place; it
# just cannot be spelled as an eager assignment.
PROFILE_DIR := `bash -c 'source scripts/build/cargo.sh && nros_cargo_target_profile_dir 2>/dev/null' || true`

# Refuse to build a path out of an unresolved profile dir. EVERY recipe that
# interpolates PROFILE_DIR / FIXTURE_TARGET must dep this first, so a stale CLI
# yields the actionable "run just setup-cli" message instead of a confusing
# "kernel not found" (issue 0430 — test-wcet / test-lan9118 / _run-qemu-mps2, and
# so its talker/listener/rtic-* callers, lacked it and got the confusing path).
#
# NB: PROFILE_DIR / FIXTURE_TARGET STAY `:=` variables (not recipe-local shell) —
# FIXTURE_TARGET is interpolated into recipe-ARGUMENT positions (`_run-qemu-mps2
# (FIXTURE_TARGET / "…")`), which cargo/just resolve at PARSE time, so they cannot
# move into a recipe body. The parse-time query is made non-fatal by the trailing
# `|| true` on the backtick (empty on a stale CLI); THIS guard makes it loud at USE.
[private]
_require-profile-dir:
    #!/usr/bin/env bash
    set -e
    if [ -z "{{PROFILE_DIR}}" ]; then
        echo "qemu: cannot resolve the cargo target-profile dir — the nros CLI is" >&2
        echo "      missing or predates \`nros profile\` (phase-336)." >&2
        echo "  Run:  just setup-cli" >&2
        exit 1
    fi

# Provision the project-local patched qemu-system-arm under build/qemu.
#
# Phase 187.6: a thin caller of `nros setup` — fetches the prebuilt patched
# qemu (built from the NEWSLabNTU/qemu `nano-ros-v11.0.0-patches` fork) from the
# nano-ros-sdk Releases into build/qemu, where the test harness already looks
# (`nros_tests::qemu::qemu_system_arm_path`). The version comes from
# nros-sdk-index.toml — no duplicated pin here. On a host with no prebuilt,
# `nros setup` builds from the index's `[tool.qemu.source]` recipe into the same
# prefix (no 2.7 GB submodule build for everyone else). Requires the nros CLI
# (`just setup base` builds it).
[group("setup")]
setup-qemu:
    #!/usr/bin/env bash
    set -euo pipefail
    prefix={{QEMU_PREFIX}}
    bin={{QEMU_BIN}}
    if [ -x "$bin" ]; then
        echo "qemu: $bin present"
        "$bin" --version | head -1
        exit 0
    fi
    source scripts/build/cargo.sh
    nros="$(nros_cli_bin)"
    "$nros" setup --tool qemu --prefix "$prefix" --index "$(pwd)/nros-sdk-index.toml"
    "$bin" --version | head -1

# Print the path to the patched qemu-system-arm binary. Empty if
# `just qemu setup-qemu` hasn't been run.
qemu-bin:
    @echo {{QEMU_BIN}}

# Run all qemu-baremetal tests with the patched qemu-system-arm.
# Wraps `just qemu test` with QEMU_SYSTEM_ARM exported so the test
# runner picks the project-local build instead of system qemu.
[group("debug")]
test-patched verbose="":
    @just qemu setup-qemu
    QEMU_SYSTEM_ARM={{QEMU_BIN}} just qemu test {{verbose}}

# Build QEMU ARM bare-metal examples
[group("main")]
build:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    # Phase 214.J.2 — guard `nros sync` against stale `generated/` after trait-
    # surface drift (see scripts/build/codegen-stamp.sh).
    source scripts/build/codegen-stamp.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    nros_cli="$(nros_cli_bin)"
    # Phase 214.I.2 — fail-loud prereq guard: the loops below invoke
    # `nros sync`, absent from the shipped 0.3.7 release. Probe once;
    # emit `[PREREQ]` + exit 0 if missing.
    nros_require_sync "$nros_cli"
    echo "Building QEMU ARM examples..."
    # phase-340 P2 — `qemu-arm-baremetal` is a migrated platform (phase-226.D,
    # the first one), so its default-config manifest rows build into
    # `build/cargo-fixtures/qemu-arm-baremetal`. Both loops below used to run a
    # bare in-leaf `cargo build`, writing a SECOND per-leaf copy of the same
    # artifacts — the same second build path phase-340 P2 closed on freertos,
    # here re-creating `examples/qemu-arm-baremetal/**/target/`. The dir comes
    # from the resolver the manifest build and the staleness probe share, so
    # rows land where their row lands and the rowless crates join the same
    # group (bin names checked collision-free across both sets). Empty for an
    # unmigrated platform, where the leaf `target/` is still right.
    # shellcheck source=scripts/build/fixtures-target-dir.sh
    source scripts/build/fixtures-target-dir.sh
    qemu_tdir_flag="$(nros_fixture_target_dir_flag qemu-arm-baremetal "" "")"
    # phase-300 W1.1 — enumerate tracked crates via the git index (the
    # F1 `-not -path` find still DESCENDED every target*/build* tree).
    _qemu_tomls="$(git ls-files 'examples/qemu-arm-baremetal/**/Cargo.toml' | awk -F/ 'NF>=4' | sort)"
    # Issue 0100 — single sync-then-build pass. The former two-pass split existed
    # only because the Entry+Node-pkg shape had an Entry whose `[patch]` pointed
    # at a SIBLING `../<pkg>/generated/*`, so every entry had to wait for its
    # sibling pkg's codegen (pass 1) before building (pass 2). Every example is
    # now a single self-contained crate (its own `package.xml` + `generated/` +
    # `.cargo/config.toml` patch, no sibling walk-up), so each dir's own
    # `nros sync` produces exactly the `generated/` its own `cargo build` needs —
    # sync + build fold into one pass.
    for toml in $_qemu_tomls; do
        dir="$(dirname "$toml")"
        if [ -f "$dir/package.xml" ]; then
            echo "  sync $dir"
            NROS_REPO_DIR="$PWD" nros_codegen_stamp_check_or_wipe "$dir"
            NROS_REPO_DIR="$PWD" "$nros_cli" sync "$dir" >/dev/null
            NROS_REPO_DIR="$PWD" nros_codegen_stamp_write "$dir"
        fi
        echo "  build $dir"
        (cd "$dir" && cargo build $cargo_profile_args $qemu_tdir_flag)
    done
    # Phase 131.B — bench/test-fixture binaries that used to live under
    # examples/qemu-arm-baremetal/rust/{core,standalone,zenoh/large-msg-test}
    # now sit under packages/testing/.
    for dir in \
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu \
        packages/testing/nros-tests/bins/lan9118-qemu \
        packages/testing/nros-tests/bins/logging-smoke-mps2-baremetal \
        packages/testing/nros-bench/wcet-cycles-qemu \
        packages/testing/nros-bench/large-msg-baremetal; do
        echo "  build $dir"
        # Same as the example loop: bins with a package.xml (e.g. cdr-roundtrip-qemu
        # path-deps builtin_interfaces/std_msgs) need codegen before cargo resolves
        # the generated/ [patch.crates-io] path.
        if [ -f "$dir/package.xml" ]; then
            NROS_REPO_DIR="$PWD" nros_codegen_stamp_check_or_wipe "$dir"
            NROS_REPO_DIR="$PWD" "$nros_cli" sync "$dir" >/dev/null
            NROS_REPO_DIR="$PWD" nros_codegen_stamp_write "$dir"
        fi
        (cd "$dir" && cargo build $cargo_profile_args $qemu_tdir_flag)
    done
    # issue 0488 residue 1 — qemu-smoltcp-bridge is a `[[fixture]]` row now, so
    # the lane above already built it into the shared group. The bare in-leaf
    # build that used to be here wrote a second copy nothing reads.

# Build all example binaries used as test fixtures (bare-metal Rust).
# Skips cleanly if the ARM cross-toolchain is missing.
[group("full-matrix")]
build-fixtures:
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh qemu build-fixtures "just build qemu"
    set -e
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    source scripts/build/lane-skip.sh   # issue 0599 — SKIPPED, not OK
    nros_lane_platform qemu                # phase-411 W2 — named `qemu` must work
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        nros_lane_skip "arm-none-eabi-gcc not found — install the ARM bare-metal toolchain"
    fi
    # Issue 0483 — the emulator suite needs libzenohpico.a, and building fixtures
    # without it produced the worst possible outcome: this recipe succeeded and
    # all 16 `emulator.rs` tests then SKIPPED, so the lane reported a green that
    # meant "did not run". Measured: 0 passed/16 skipped in ~1 s, versus 16
    # passed/0 skipped in 116 s once the library existed.
    #
    # Built here rather than left to `just qemu setup`, because "I built the
    # fixtures" is exactly when someone expects the suite to be runnable. The
    # script is idempotent and returns immediately when the archive is current.
    source scripts/build/build-root.sh
    zpdir="$(nros_build_dir "$NROS_KIND_QEMU_ZENOH_PICO")"
    if [ ! -f "$zpdir/libzenohpico.a" ]; then
        echo "Building zenoh-pico for ARM (required by the emulator suite)..."
        ./scripts/qemu/build-zenoh-pico.sh
    fi
    echo "Building QEMU bare-metal test fixtures..."
    scripts/build/fixtures-build.sh qemu-arm-baremetal rust
    # issue 0488 residue 1 — "remains ad-hoc until reference package coverage
    # moves into the fixture manifest" is done: it has a row, so the
    # `fixtures-build.sh` call above builds it with a coordinate and a group.

# Phase-395 W0.5 — the two lane verbs every other platform module has.
#
# `nightly.yml` runs `just <plat> build-all || just <plat> build-examples ||
# just <plat> build`. `qemu` was the ONLY module missing both, so that chain
# silently fell through to the lightest `build` for this platform alone — the
# heavy fixture build QEMU e2e actually consumes never ran on the nightly, and
# the two "does not contain recipe" errors were swallowed by the `||`. Silent
# narrowing: reduced coverage reported as success.
#
# `build-all` mirrors freertos/threadx/nuttx/esp32 — the fixture build is the
# whole of it here, because `build-fixtures` already covers the example
# binaries this platform ships.
[group("full-matrix")]
build-all: build-fixtures
    @echo "QEMU bare-metal build-all completed."

# Alias kept because the nightly chain names it and a missing verb degrades
# silently rather than failing.
[group("full-matrix")]
build-examples: build-fixtures
    @echo "QEMU bare-metal build-examples completed."

# Run all QEMU tests (non-networked)
[group("main")]
test verbose="": build-fixtures
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh qemu test "just test qemu"
    set +e
    failed=0
    just init-test-logs
    just qemu test-basic {{verbose}} || failed=1
    # `test-wcet` is deliberately NOT here. The bench refuses to emit numbers on
    # QEMU because the DWT cycle counter does not increment there, and a zero
    # WCET is indistinguishable from "this operation is free" — the most
    # optimistic value there is, which always errs toward `schedulable`
    # (issues 0403 / 0259). That refusal is correct, and it means the bench can
    # NEVER pass under emulation.
    #
    # So including it in the QEMU aggregate made this lane red by construction:
    # the nightly `qemu` job failed on it every run, which is how a permanently
    # red lane stops carrying signal about anything else in it (phase-395 W0.5).
    # WCET belongs to the hardware lane; run `just qemu test-wcet` directly on a
    # board.
    just qemu test-lan9118 {{verbose}} || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some QEMU tests failed."
        exit 1
    else
        echo "All QEMU tests passed!"
    fi

# Phase 226.D — shared fixture-only Cargo target dir for qemu-arm-baremetal
# Rust rows. `build-fixtures` (scripts/build/fixtures-build.sh) builds every
# eligible row into this tree instead of each example's local target/, so the
# kernel paths below resolve here. Mirrors fixture_shared_target_dir() in
# packages/testing/nros-tests/src/fixtures/binaries/mod.rs.
FIXTURE_TARGET := absolute_path("build/cargo-fixtures/qemu-arm-baremetal") / "thumbv7m-none-eabi" / PROFILE_DIR

# Run basic QEMU test (nros serialization on Cortex-M3)
[group("debug")]
test-basic verbose="": _require-profile-dir build-fixtures
    ./tests/run-test.sh --name qemu-basic --log {{LOG_DIR}}/latest/qemu-basic.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel {{FIXTURE_TARGET}}/qemu-rs-test

# Run WCET benchmark on QEMU (DWT cycle counter)
[group("debug")]
test-wcet verbose="": _require-profile-dir build-fixtures
    ./tests/run-test.sh --name qemu-wcet-bench --log {{LOG_DIR}}/latest/qemu-wcet-bench.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine lm3s6965evb -nographic \
            -semihosting-config enable=on,target=native \
            -kernel {{FIXTURE_TARGET}}/qemu-rs-wcet-bench

# Lift the WCET bench's machine-readable half out of its log (issue 0403).
#
# Separate from `test-wcet` because the two answer different questions: the
# bench answers "did a measurement happen", this answers "where did it go".
# Keeping them apart also means a refused run — which is EVERY run on QEMU,
# since the DWT does not count there — reports its own reason rather than
# being swallowed by a conversion step.
[group("debug")]
wcet-artifact log=(LOG_DIR / "latest/qemu-wcet-bench.log"):
    python3 scripts/bench/wcet-log-to-json.py "{{log}}" \
        "$(dirname "{{log}}")/qemu-wcet-bench.json"

# Run LAN9118 Ethernet driver test (mps2-an385)
[group("debug")]
test-lan9118 verbose="": _require-profile-dir build-fixtures
    ./tests/run-test.sh --name qemu-lan9118 --log {{LOG_DIR}}/latest/qemu-lan9118.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine mps2-an385 -nographic \
            -semihosting-config enable=on,target=native \
            -kernel {{FIXTURE_TARGET}}/qemu-rs-lan9118

# Build the Phase 216.B generated-entry RTIC E2E fixture.
[group("debug")]
build-rtic-main-e2e:
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    # issue 0488 residue 1 — this leaf IS a `[[fixture]]` row
    # (qemu-arm-baremetal/rust), and that platform is migrated, so the fixture
    # lane compiles it into `build/cargo-fixtures/<slug>`. The bare in-leaf build
    # here wrote the leaf's own `target/` instead, and `test-rtic-main-e2e` then
    # `-kernel`ed THAT — a second copy nothing else reads, and a path that
    # disagrees with where the lane writes.
    source scripts/build/fixtures-target-dir.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    rtic_tdir_flag="$(nros_fixture_target_dir_flag qemu-arm-baremetal "" "")"
    cd packages/testing/nros-tests/bins/rtic-run-plan-e2e
    # shellcheck disable=SC2086
    cargo build $cargo_profile_args $rtic_tdir_flag --quiet

# Run the Phase 216.B generated-entry RTIC E2E fixture on QEMU MPS2-AN385.
[group("debug")]
test-rtic-main-e2e verbose="": _require-profile-dir build-rtic-main-e2e
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p {{LOG_DIR}}/latest
    source scripts/dev/zenohd.sh
    source scripts/build/lane-skip.sh   # issue 0599 — SKIPPED, not OK
    if ! zenohd_bin="$(nros_zenohd_bin)"; then
        nros_lane_skip "the ROS zenoh router is absent — install \`ros-<distro>-rmw-zenoh-cpp\` or set NROS_RMW_ZENOHD (phase-362)"
    fi
    # issue 0488 residue 1 — the kernel path comes from the same
    # `nros_fixture_group` answer `build-rtic-main-e2e` builds with.
    source scripts/build/fixtures-target-dir.sh
    rtic_artifacts="$(nros_fixture_row_artifact_dir packages/testing/nros-tests/bins/rtic-run-plan-e2e qemu-arm-baremetal "" "")"
    zenoh_log={{LOG_DIR}}/latest/qemu-rtic-main-e2e-zenohd.log
    # phase-362 — `rmw_zenohd` is configured by env, not argv. Backgrounded with
    # a log here, so it uses the binary + override directly rather than
    # `nros_router_exec` (which execs).
    ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:10200"];scouting/multicast/enabled=false' \
        "$zenohd_bin" > "$zenoh_log" 2>&1 &
    zenoh_pid=$!
    trap 'kill "$zenoh_pid" 2>/dev/null || true; wait "$zenoh_pid" 2>/dev/null || true' EXIT
    sleep 0.5
    ./tests/run-test.sh --name qemu-rtic-main-e2e --log {{LOG_DIR}}/latest/qemu-rtic-main-e2e.log \
        --qemu {{ if verbose != "" { "--verbose" } else { "" } }} -- \
        {{ if path_exists(QEMU_BIN) == "true" { QEMU_BIN } else { "qemu-system-arm" } }} -cpu cortex-m3 -machine mps2-an385 -nographic \
            -icount shift=auto \
            -semihosting-config enable=on,target=native \
            -kernel "$rtic_artifacts/thumbv7m-none-eabi/{{PROFILE_DIR}}/qemu-rtic-main-e2e" \
            -nic user,model=lan9118

# Run all QEMU tests including networked E2E and RTIC
[group("full-matrix")]
test-all verbose="":
    #!/usr/bin/env bash
    set -e
    source scripts/build/cargo.sh
    cargo_nextest_args=($(nros_cargo_nextest_args))
    args=(-p nros-tests --no-fail-fast \
          -E 'binary(emulator) or binary(large_msg)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${cargo_nextest_args[@]}" "${args[@]}"

# Check if QEMU is installed
check:
    @which qemu-system-arm > /dev/null || (echo "Error: qemu-system-arm not found. Install with: nros setup --system" && exit 1)
    @echo "QEMU ARM is installed"

# CI: check + test
[group("full-matrix")]
ci: check test

# Setup QEMU TAP network bridge (requires sudo)
[group("setup")]
setup-network:
    sudo ./scripts/qemu/setup-network.sh

# Teardown QEMU TAP network bridge (requires sudo)
teardown-network:
    sudo ./scripts/qemu/setup-network.sh --down

# Show QEMU network status
status-network:
    ./scripts/qemu/setup-network.sh --status

# Test QEMU zenoh-pico communication (requires zenohd + TAP network)
[group("debug")]
test-zenoh: build
    #!/usr/bin/env bash
    set -e
    echo "============================================"
    echo "  QEMU zenoh-pico Communication Test"
    echo "============================================"
    echo ""
    echo "Prerequisites:"
    echo "  1. TAP network: sudo ./scripts/qemu/setup-network.sh"
    echo "  2. zenohd running: ZENOH_CONFIG_OVERRIDE='listen/endpoints=[\"tcp/0.0.0.0:7447\"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd"
    echo ""
    echo "This test requires QEMU 7.0+ for reliable TAP networking."
    echo "Ubuntu 22.04 ships QEMU 6.2.0 which has TAP issues."
    echo ""
    echo "Current QEMU version:"
    qemu-system-arm --version | head -1
    echo ""
    echo "To run manually:"
    echo "  Terminal 1: ZENOH_CONFIG_OVERRIDE='listen/endpoints=[\"tcp/0.0.0.0:7447\"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd"
    echo "  Terminal 2: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary {{FIXTURE_TARGET}}/qemu-bsp-talker"
    echo "  Terminal 3: ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu1 --binary {{FIXTURE_TARGET}}/qemu-bsp-listener"
    echo ""
    echo "Binaries built at (Phase 226.D shared fixture target dir):"
    echo "  {{FIXTURE_TARGET}}/qemu-bsp-talker"
    echo "  {{FIXTURE_TARGET}}/qemu-bsp-listener"
    echo ""
    echo "Note: Automated test not yet implemented (requires QEMU 7.0+)"
    echo "============================================"

# Launch QEMU mps2-an385 with networking (example)
run-networked TAP="tap-qemu0" IP="192.0.2.10" BINARY="":
    ./scripts/qemu/launch-mps2-an385.sh --tap {{TAP}} --ip {{IP}} --binary {{BINARY}}

# Install QEMU bare-metal prerequisites (ARM zenoh-pico + patched
# qemu-system-arm under build/qemu/). Building patched QEMU is
# idempotent and short-circuits when build/qemu/bin/qemu-system-arm
# is newer than every patch file.
#
# Phase 213.A.3 — this recipe covers BOTH boards built on this toolchain:
#   * `[board.qemu-arm-baremetal]` (QEMU + arm-none-eabi-gcc)
#   * `[board.mps2-an385]` (same toolchain; real hw uses openocd separately)
# There is no `just mps2-an385 setup` sibling — the recipe is shared because
# the SDK packages overlap 1:1 and mps2-an385's only divergence is `openocd`
# for real-hw flashing (out-of-tree concern). Users targeting mps2-an385
# should run `just qemu setup` AND `nros setup mps2-an385 --tool openocd`
# (the latter when flashing real hardware).
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    # Phase 197.4/196.9 — provision the board's sources (arm-gcc, qemu dist, +
    # zenoh-pico/mbedtls via the default zenoh RMW) via nros before the arm
    # zenoh-pico build needs them. Then the patched-qemu + zenoh-pico-arm builds.
    # Phase 208 (post-CI dispatch) — `examples/qemu-arm-baremetal/rust/talker-xrce/`
    # (Phase 207) needs micro-cdr + micro-xrce-dds-client sources too, otherwise
    # `nros-rmw-xrce-cffi/build.rs` panics "submodule not initialised". Provision
    # both RMW source sets in the same step so `just qemu build` covers every
    # in-tree bare-metal example without a separate setup invocation.
    source scripts/build/cargo.sh
    "$(nros_cli_bin)" setup qemu-arm-baremetal --rmw zenoh
    "$(nros_cli_bin)" setup --source micro-cdr --source micro-xrce-dds-client
    # build-zenoh-pico.sh picks `arm-none-eabi-gcc` off PATH; the nros-provisioned
    # arm-gcc 13.2 (newlib headers — apt's gcc-arm-none-eabi has none, so its
    # bare-metal compile fails `stdlib.h: No such file`) lands in ~/.nros/sdk/*/bin,
    # which setup.bash globs onto PATH. Source it before the arm build (same as the
    # build step + how nuttx already picks up the newlib toolchain).
    source setup.bash
    just qemu build-zenoh-pico
    just qemu setup-qemu

# Diagnose QEMU bare-metal install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    bash scripts/dev/deprecated-scope-alias.sh qemu doctor "just doctor qemu"
    set +e
    # phase-334 W2.b step 2 — path from the ONE derivation (RFC-0070 R3).
    # NOTE: `QEMU_PREFIX` at the top of this file stays a literal on purpose —
    # it is a PARSE-TIME `absolute_path()`, and calling a bash function there
    # would need a `shell()` on every justfile parse. Recorded as a known
    # exception in phase-334 W2.b, not an oversight.
    source scripts/build/build-root.sh
    zpdir="$(nros_build_dir "$NROS_KIND_QEMU_ZENOH_PICO")"
    fail=0
    if [ -f "$zpdir/libzenohpico.a" ]; then
        echo "  [OK] qemu-zenoh-pico: $zpdir/libzenohpico.a"
    else
        echo "  [MISSING] qemu-zenoh-pico (run: just qemu setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: nros setup --tool arm-none-eabi-gcc — prebuilt, no sudo; or apt gcc-arm-none-eabi) — 0368 F2"
        fail=1
    fi
    if [ -x {{QEMU_BIN}} ]; then
        ver=$({{QEMU_BIN}} --version 2>/dev/null | head -1)
        echo "  [OK] patched qemu-system-arm: {{QEMU_BIN}} ($ver)"
    else
        echo "  [MISSING] patched qemu-system-arm (run: just qemu setup-qemu)"
        fail=1
    fi
    if command -v qemu-system-arm &>/dev/null; then
        ver=$(qemu-system-arm --version 2>/dev/null | head -1 | sed -E 's/^[^0-9]*([0-9]+\.[0-9]+).*/\1/')
        major=${ver%%.*}
        minor=${ver##*.}
        # Phase 127.B.5 — `-netdev dgram,local.type=unix,...` (the
        # supported workaround for QEMU 6.2's broken cross-process
        # `socket,mcast=` delivery) was introduced in QEMU 7.2.
        # NuttX DDS / RTPS multi-instance tests need it.
        if [ -n "$ver" ] && { [ "$major" -gt 7 ] || { [ "$major" -eq 7 ] && [ "$minor" -ge 2 ]; }; }; then
            echo "  [OK] system qemu-system-arm ($ver, fallback path; >= 7.2 — supports -netdev dgram unix)"
        elif [ -x {{QEMU_BIN}} ]; then
            # Phase 143 — patched build is the primary path and is
            # already OK above; demote the system-binary warning to
            # an informational note so users aren't pressured to
            # touch system packages.
            echo "  [INFO] system qemu-system-arm ($ver) < 7.2 — patched build at {{QEMU_BIN}} is the active path; nothing to do."
        else
            echo "  [WARN] system qemu-system-arm ($ver) < 7.2 AND patched build absent."
            echo "         Primary remedy (no sudo, portable across distros):"
            echo ""
            echo "           just qemu setup-qemu"
            echo ""
            echo "         (builds third-party/qemu/qemu @ stable-11.0 with the"
            echo "         LAN9118 RX-flush patch into build/qemu/bin/qemu-system-arm;"
            echo "         picked up automatically by the test harness via Phase 143's"
            echo "         qemu_system_arm_path() resolver — no env-var needed.)"
            echo ""
            echo "         Fallback (system-wide, requires sudo):"
            if [ -f /etc/os-release ] && grep -q '^ID=ubuntu' /etc/os-release; then
                codename=$(. /etc/os-release; echo "$VERSION_CODENAME")
                echo "           Ubuntu detected ($codename). Canonical server-backports PPA:"
                echo "             sudo add-apt-repository ppa:canonical-server/server-backports"
                echo "             nros setup --system   (composes the install command for this host)"
                echo "           https://launchpad.net/~canonical-server/+archive/ubuntu/server-backports"
            else
                echo "           Build from source: https://www.qemu.org/download/#source"
            fi
        fi
    else
        echo "  [MISSING] system qemu-system-arm (apt path). The patched"
        echo "            build at {{QEMU_BIN}} is the recommended path —"
        echo "            run \`just qemu setup-qemu\` to produce it."
        # No fail bump: if the patched build is in place, the system
        # binary is unused. The patched-binary check above already
        # bumped fail when {{QEMU_BIN}} was absent.
    fi
    exit $fail

# Build zenoh-pico for ARM Cortex-M3 (required for QEMU examples)
[group("examples")]
build-zenoh-pico:
    @./scripts/qemu/build-zenoh-pico.sh

# Clean zenoh-pico ARM build
[group("main")]
clean-zenoh-pico:
    ./scripts/qemu/build-zenoh-pico.sh --clean

# Show QEMU help
help:
    @echo "QEMU Bare-Metal Testing"
    @echo "======================="
    @echo ""
    @echo "Prerequisites:"
    @echo "  1. Install QEMU: nros setup --system   (or just qemu setup for the nros dist)"
    @echo "  2. Set up network: sudo ./scripts/qemu/setup-network.sh"
    @echo ""
    @echo "Build & Test (non-networked):"
    @echo "  just qemu build              # Build all QEMU examples"
    @echo "  just qemu test               # Run all QEMU tests (no network)"
    @echo "  just qemu test-basic         # Run basic serialization test"
    @echo "  just qemu test-wcet          # Run WCET benchmark"
    @echo "  just qemu test-lan9118       # Run LAN9118 driver test"
    @echo ""
    @echo "Zenoh-pico (networked):"
    @echo "  just qemu build-zenoh-pico   # Build zenoh-pico for ARM Cortex-M3"
    @echo "  just qemu test-zenoh         # Show zenoh test instructions"
    @echo ""
    @echo "Network Setup:"
    @echo "  just qemu setup-network      # Create TAP bridge (requires sudo)"
    @echo "  just qemu teardown-network   # Remove TAP bridge (requires sudo)"
    @echo "  just qemu status-network     # Show network status"
    @echo ""
    @echo "Manual QEMU Launch:"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --binary app.elf"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --tap tap-qemu0 --binary app.elf"
    @echo "  ./scripts/qemu/launch-mps2-an385.sh --gdb --binary app.elf"
    @echo ""
    @echo "For more details: ./scripts/qemu/launch-mps2-an385.sh --help"

# Clean QEMU bare-metal build artifacts
[group("main")]
clean: clean-zenoh-pico
    #!/usr/bin/env bash
    # phase-300 W1.1 — tracked-crate enumeration (see build recipe).
    for toml in $(git ls-files 'examples/qemu-arm-baremetal/**/Cargo.toml' | awk -F/ 'NF>=4' | sort); do
        rm -rf "$(dirname "$toml")/target"
    done
    # Phase 131.B — relocated bare-metal bench/fixture crates.
    for dir in \
        packages/testing/nros-tests/bins/cdr-roundtrip-qemu \
        packages/testing/nros-tests/bins/lan9118-qemu \
        packages/testing/nros-tests/bins/logging-smoke-mps2-baremetal \
        packages/testing/nros-bench/wcet-cycles-qemu \
        packages/testing/nros-bench/large-msg-baremetal; do
        rm -rf "$dir/target"
    done
    rm -rf packages/testing/qemu-smoltcp-bridge/target
    # Phase 226.D — shared fixture-only Cargo target dir.
    rm -rf build/cargo-fixtures/qemu-arm-baremetal
    echo "QEMU bare-metal build artifacts cleaned"

# =============================================================================
# Run examples (build + launch QEMU MPS2-AN385 with slirp networking)
# =============================================================================

# QEMU ignores SIGINT — run in background with trap for Ctrl-C
# Phase 226.D — depends on build-fixtures so the binary resolves under the
# shared fixture target dir ({{FIXTURE_TARGET}}); callers pass paths there.
[no-exit-message]
_run-qemu-mps2 binary: _require-profile-dir build-fixtures
    #!/usr/bin/env bash
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "{{binary}}" \
        -nic user,model=lan9118 &
    PID=$!
    wait $PID

# Run bare-metal talker on QEMU
talker: (_run-qemu-mps2 (FIXTURE_TARGET / "qemu-bsp-talker"))

# Run bare-metal listener on QEMU
listener: (_run-qemu-mps2 (FIXTURE_TARGET / "qemu-bsp-listener"))

# Run RTIC service server on QEMU
rtic-service-server: (_run-qemu-mps2 (FIXTURE_TARGET / "qemu-rtic-service-server"))

# Run RTIC service client on QEMU
rtic-service-client: (_run-qemu-mps2 (FIXTURE_TARGET / "qemu-rtic-service-client"))

# Run RTIC action server on QEMU
rtic-action-server: (_run-qemu-mps2 (FIXTURE_TARGET / "qemu-rtic-action-server"))

# Run RTIC action client on QEMU
rtic-action-client: (_run-qemu-mps2 (FIXTURE_TARGET / "qemu-rtic-action-client"))

# Start zenohd router for bare-metal QEMU examples (allocator RTIC pubsub port 10200)
zenohd:
    #!/usr/bin/env bash
    set -e
    source scripts/dev/zenohd.sh
    nros_router_exec tcp/127.0.0.1:10200
