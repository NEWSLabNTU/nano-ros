#!/usr/bin/env bash
# Phase 196.6 — per-platform dependency-chain validation (light, no full builds).
#
# For each (board, rmw) cell it proves the dep chain *resolves* — it does NOT
# compile every platform (that's the sparse `just build-all` / `zephyr-dual-line`
# lanes). Per cell:
#   1. toolchain side : `nros setup <board> --rmw <rmw> --dry-run`
#                       (the [board.*]/[rmw.*] index wiring → prebuilt host tools)
#   2. codegen        : `nros generate-rust` in the example (produces the
#                       generated/ interface path-crates the example deps on)
#   3. crate/feature  : `cargo tree` from the example dir (its .cargo/config.toml
#                       [patch.crates-io] + `target=` apply) — resolution only.
#
# Catches a broken feature/crate/toolchain wiring (missing optional dep, a
# feature that won't resolve on a target, a board→toolchain typo) in seconds.
#
# Preconditions (fail loud — never silently pass, per CLAUDE.md):
#   - ROS 2 sourced: `nros generate-rust` resolves std_msgs's .msg via
#     AMENT_PREFIX_PATH. `source /opt/ros/<distro>/setup.bash` first.
#   - $NROS points at the `nros` CLI. Phase 218 resolution order:
#     $NROS env → packages/cli/target/release/nros (in-tree build via
#     `just setup-cli`) → `nros` on PATH → ~/.nros/bin/nros
#     (transitional fallback). The packages/codegen submodule that
#     pre-218 carried the in-tree codegen is retired — the CLI now
#     lives at packages/cli/ as a sub-workspace.
#
# Usage: source /opt/ros/humble/setup.bash && scripts/ci/dep-chain-check.sh
set -uo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.." # repo root

# Phase 218 lookup: prefer the per-checkout sub-workspace binary, fall
# back to PATH, then to the transitional ~/.nros/bin location.
if [ -n "${NROS:-}" ]; then
    : # already set; respect the override
elif [ -x "packages/cli/target/release/nros" ]; then
    NROS="$(pwd)/packages/cli/target/release/nros"
elif command -v nros >/dev/null 2>&1; then
    NROS="$(command -v nros)"
else
    NROS="${NROS_HOME:-$HOME/.nros}/bin/nros"
fi
INDEX="${NROS_SDK_INDEX:-nros-sdk-index.toml}"

# --- preconditions ---
if [ -z "${AMENT_PREFIX_PATH:-}" ]; then
    echo "ERROR: ROS 2 not sourced (AMENT_PREFIX_PATH unset). 'nros generate-rust'" >&2
    echo "       needs std_msgs's .msg defs. Run: source /opt/ros/<distro>/setup.bash" >&2
    exit 1
fi
if [ ! -x "$NROS" ]; then
    echo "ERROR: nros CLI not found at '$NROS'. Run 'just setup-cli' (Phase 218)" >&2
    echo "       or set \$NROS to a built binary." >&2
    exit 1
fi
# Absolute — used inside `cd "$ex"` subshells below.
NROS="$(cd "$(dirname "$NROS")" && pwd)/$(basename "$NROS")"

# --- the board × rmw matrix (rust talker; resolvable RMWs only) ---
# Skipped on purpose: native+cyclonedds (pending 171.C.1), zephyr (the
# west/cmake build is covered by zephyr-dual-line, not this cargo-tree lane).
CELLS=(
    "native:zenoh"
    "native:xrce"
    "esp32:zenoh"
    "qemu-arm-baremetal:zenoh"
    "qemu-arm-freertos:zenoh"
    "qemu-arm-nuttx:zenoh"
    "qemu-esp32-baremetal:zenoh"
    "qemu-riscv64-threadx:zenoh"
    "stm32f4:zenoh"
    "threadx-linux:zenoh"
)

pass=0
fail=0
failed_cells=()

for cell in "${CELLS[@]}"; do
    board="${cell%%:*}"
    rmw="${cell##*:}"
    ex="examples/${board}/rust/talker"
    echo "::group::${board} / ${rmw}"
    cell_ok=1

    if [ ! -f "$ex/Cargo.toml" ]; then
        echo "  [FAIL] no example at $ex"
        cell_ok=0
    else
        # 1. the actual user step: `nros setup <board>` — provisions the board's
        #    prebuilt toolchains AND its source submodules (e.g. nuttx-libc the
        #    example path-deps). NOT --dry-run: the user does not hand-checkout
        #    submodules, so neither does CI — if a build needs a source, the index
        #    + `nros setup` must provide it (that's part of what this validates).
        #    The store dedups across cells, so toolchains download once.
        if "$NROS" setup "$board" --rmw "$rmw" --index "$INDEX"; then
            echo "  [ok] nros setup (toolchains + sources provisioned)"
        else
            echo "  [FAIL] nros setup $board --rmw $rmw"
            cell_ok=0
        fi

        # 2. codegen the example's interface crates.
        #    NROS_SKIP_VERSION_CHECK=1: this lane validates dep-chain *resolution*
        #    only (no compile, no runtime), so the abi_guard's stale-standalone-
        #    lockfile mismatch is a false positive here — known-issue #12: the
        #    committed example Cargo.locks still pin nros-core 0.1.0 (the 218.J
        #    0.1.0->0.5.0 bump never propagated to standalone locks), tripping the
        #    guard even though the real source tree is 0.5.0. Bypass so codegen
        #    emits generated/ for the cargo-tree step.
        if ( cd "$ex" && NROS_SKIP_VERSION_CHECK=1 "$NROS" generate-rust >/dev/null 2>&1 ); then
            : # generated/ now present
        else
            echo "  [FAIL] nros generate-rust (codegen — ROS sourced? msg deps resolvable?)"
            cell_ok=0
        fi

        # 3. crate/feature/target dep chain — resolution only (no compile).
        #    Run from the example dir so its .cargo/config.toml patch + target apply.
        #    RMW-selectable examples expose `rmw-<rmw>`; others hardcode the
        #    backend (resolve with default features then).
        feat_args=()
        if ( cd "$ex" && cargo metadata --no-deps --format-version 1 2>/dev/null \
                | grep -q "\"rmw-${rmw}\"" ); then
            feat_args=(--no-default-features --features "rmw-${rmw}")
        fi
        if ( cd "$ex" && cargo tree "${feat_args[@]}" -e no-dev >/dev/null 2>&1 ); then
            echo "  [ok] crate/feature dep chain resolves (${feat_args[*]:-default features})"
        else
            echo "  [FAIL] cargo tree did not resolve (${feat_args[*]:-default features}):"
            ( cd "$ex" && cargo tree "${feat_args[@]}" -e no-dev 2>&1 | grep -iE 'error|failed' | head -3 | sed 's/^/      /' )
            cell_ok=0
        fi
    fi

    if [ "$cell_ok" -eq 1 ]; then
        pass=$((pass + 1)); echo "  PASS"
    else
        fail=$((fail + 1)); failed_cells+=("$cell")
    fi
    echo "::endgroup::"
done

echo ""
echo "dep-chain: $pass passed, $fail failed (of $((pass + fail)) cells)"
if [ "$fail" -ne 0 ]; then
    printf '  FAILED: %s\n' "${failed_cells[@]}"
    exit 1
fi
