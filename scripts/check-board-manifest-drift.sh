#!/usr/bin/env bash
#
# Phase 215.F.2 — board-crate manifest drift gate (CI/lint surface).
#
# Every `packages/boards/nros-board-*` crate that carries BOTH a
# `board.cmake` sidecar (the cmake face consumed by
# `nano_ros_use_board()`) AND a `[package.metadata.nros.board]` table
# (the Cargo/Rust face consumed by `nros board info`) MUST keep the two
# faces in lock-step. This gate runs `nros board info <name>
# --check-drift` for every such crate and fails `just check` on any
# field-by-field drift.
#
# This is the lint-surface sibling of the
# `packages/cli/nros-cli-core/tests/phase215_f_manifest_drift.rs`
# integration test (which runs inside the `packages/cli/` sub-workspace
# CI). Both call the SAME `compute_drift` implementation — the test
# walks every board in its own harness; this gate reuses the shipped
# `nros` binary so a plain `just check` catches drift without building
# the CLI test suite.
#
# Boards carrying only ONE face (bare Rust-only boards w/o `board.cmake`,
# or boards where Phase 215.A hasn't landed) are skipped — there is
# nothing to drift against. Mirrors `scripts/check-board-abi-mirror.sh`
# / `scripts/check-profile-board-mirror.sh`. Hooked from `just check`.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOARDS_DIR="$ROOT/packages/boards"

# Resolve the `nros` binary the same way every other recipe does
# ($NROS_CLI → PATH → packages/cli/target/release/nros → ~/.nros/bin).
# shellcheck source=scripts/build/cargo.sh
source "$ROOT/scripts/build/cargo.sh"

NROS_BIN="$(nros_cli_bin 2>/dev/null || true)"
if [[ -z "$NROS_BIN" || ! -x "$NROS_BIN" ]]; then
    # No built CLI on this checkout — nothing to run the gate with.
    # The `packages/cli/` sub-workspace's phase215_f integration test
    # still covers the audit in that CI lane; skip rather than fail a
    # Rust-only `just check`.
    echo "skip: nros CLI not built (run \`just setup-cli\`); board manifest" \
         "drift gate covered by packages/cli phase215_f test"
    exit 0
fi

# Capability probe (mirrors `nros_cli_ws_sync_available` in cargo.sh): a
# pre-215.C `nros` (e.g. a stale ~/.nros/bin transitional install) has no
# `board info` verb, so the gate can't run. Skip with a rebuild hint
# rather than fail `just check` on a clap "unrecognized subcommand" error
# — the verb is the gate's contract, not drift.
if ! "$NROS_BIN" board info --help >/dev/null 2>&1; then
    echo "skip: resolved nros ($NROS_BIN) predates \`board info\`" \
         "(Phase 215.C) — rebuild the in-tree CLI via \`just setup-cli\`," \
         "or set \$NROS_CLI to a current binary"
    exit 0
fi

if [[ ! -d "$BOARDS_DIR" ]]; then
    echo "error: no $BOARDS_DIR directory" >&2
    exit 1
fi

audited=0
fail=0
for board_cmake in "$BOARDS_DIR"/nros-board-*/board.cmake; do
    # Glob with no match expands literally — guard it.
    [[ -e "$board_cmake" ]] || continue
    crate_dir="$(dirname "$board_cmake")"
    dir_name="$(basename "$crate_dir")"
    name="${dir_name#nros-board-}"

    # `info --check-drift` exits non-zero on drift; its JSON dump goes to
    # stdout, so capture it and only surface on failure to keep the gate
    # quiet on the happy path.
    if out="$("$NROS_BIN" board info "$name" --check-drift --workspace "$ROOT" 2>&1)"; then
        audited=$((audited + 1))
    else
        echo "drift: board manifest drift in $dir_name:" >&2
        echo "$out" >&2
        fail=1
    fi
done

if (( fail )); then
    exit 1
fi

if (( audited == 0 )); then
    echo "board manifest drift gate: no board crate carries both board.cmake" \
         "and [package.metadata.nros.board] yet — nothing to audit"
    exit 0
fi

echo "board manifest drift gate clean: $audited board crate(s) — Cargo.toml" \
     "and board.cmake agree field-by-field"
