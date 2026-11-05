#!/usr/bin/env bash
# Phase 196.7 — user-journey check: a `nros new` project resolves end-to-end via
# the source-release dependency convention (RFC-0040), not crates.io.
#
# Exercises exactly the documented out-of-tree flow:
#   1. `nros new <ws> --platform <p> --lang rust`  (scaffold a workspace, phase-531 W4;
#                                                   the node emits `version = "*"`)
#   2. NROS_REPO_DIR=<repo> + `nros configure`      (write the [patch.crates-io] block,
#                                                   the generated entry and its selection)
#   3. `cargo tree` on the generated entry          (resolution proves the patch redirects)
#
# Resolution-only (no compile / no cross build-std) — like dep-chain.yml; it
# catches the regression class this convention fixes (a scaffolded `version =
# "0.1"` crates.io dep that cannot resolve because nano-ros publishes nothing).
#
# Env:
#   NROS — path to the `nros` binary (default: resolve from PATH).
set -euo pipefail

NROS="${NROS:-nros}"
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=../lib/grep-q.sh
. "$REPO/scripts/lib/grep-q.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# An embedded cell: a cross board's generated entry is what resolves here,
# which a host cell would not exercise (no board crate, no build-std).
plat="baremetal"
ws_name="uj_ws"
# phase-531 W4 (RFC-0121 D4) — `nros new <ws>` writes a workspace whose node
# package is `<ws minus _ws>_node`, deployed by `<node>_bringup`.
name="uj_node"

echo "::group::nros new ${ws_name} --platform ${plat}"
( cd "$scratch" && "$NROS" new "$ws_name" --platform "$plat" --lang rust )
ws="$scratch/$ws_name"
proj="$ws/src/$name"
echo "::endgroup::"

# Guard: no stale crates.io `version = "0.1"` nros/board dep leaked into the scaffold.
if grep -nE '(nros|nros-board[a-z0-9-]*)\s*=\s*\{[^}]*version\s*=\s*"0\.1"' "$proj/Cargo.toml"; then
    echo "FAIL: scaffold emitted a crates.io version = \"0.1\" dep (RFC-0040 violation)" >&2
    exit 1
fi
# A node package names no board crate and no selection crate (RFC-0121 D1).
if grep -nE '^(nros-board[a-z0-9-]*|[a-z0-9_]*_nros_selection)\s*=' "$proj/Cargo.toml"; then
    echo "FAIL: the scaffolded node package names a board or selection crate (RFC-0121 D1)" >&2
    exit 1
fi

echo "::group::nros configure (NROS_REPO_DIR=${REPO})"
( cd "$ws" && NROS_REPO_DIR="$REPO" "$NROS" configure )
echo "::endgroup::"

# Issue 1265 -- the scaffolded node must be PROBED, not degraded. Before the
# template target-scoped its board, HAL and panic handler, the host probe
# compiled them too and died on `bkpt` fed to an x86 assembler; sync still
# exited 0 and left `<component>.json.unprobeable`, so every pool stayed at the
# crate default and this job PASSED. The probe's output is the artifact.
if [ ! -s "$proj/metadata/${name}.json" ] || [ -e "$proj/metadata/${name}.json.unprobeable" ]; then
    echo "FAIL: nros configure did not probe the scaffolded node (issue 1265):" >&2
    ls -la "$proj/metadata" >&2 2>/dev/null || echo "      no metadata/ dir at all" >&2
    cat "$proj/metadata/${name}.json.unprobeable" >&2 2>/dev/null || true
    exit 1
fi
echo "  [ok] nros configure probed the scaffolded node (metadata/${name}.json)"

# The managed patch block must redirect `nros` to a path.
if ! grep -q "^nros = {" "$ws/.cargo/config.toml" 2>/dev/null; then
    echo "FAIL: 'nros = {' not patched into the workspace's [patch.crates-io] block after sync" >&2
    cat "$ws/.cargo/config.toml" >&2 2>/dev/null || echo "      no .cargo/config.toml at the workspace root" >&2
    exit 1
fi

# The generated entry reaches the board, backend and edition through its
# `<entry>_nros_selection` crate, which `nros configure` generates.
entry=""
for c in "$ws"/build/*/*_entry/Cargo.toml; do
    [ -f "$c" ] && { entry="$c"; break; }
done
edir="$(dirname "${entry:-/nonexistent/x}")"
sel="$ws/generated/nros-selection/$(basename "$edir")/Cargo.toml"
if [ -z "$entry" ] || [ ! -s "$sel" ] || [ ! -s "$edir/nros-cargo.toml" ]; then
    echo "FAIL: configure wrote no generated entry, selection crate or cargo settings:" >&2
    echo "      entry=${entry:-<none>} selection=$sel" >&2
    exit 1
fi

echo "::group::cargo tree (resolution)"
if ( cd "$ws" && cargo tree -e no-dev --manifest-path "$entry" --config "$edir/nros-cargo.toml" >/dev/null 2>&1 ); then
    echo "  [ok] scaffolded workspace's entry resolves via the source-release patch block"
else
    echo "FAIL: cargo tree did not resolve the scaffolded workspace's entry:" >&2
    # Keep stderr, do not pattern-match the merged stream -- see the note at
    # the same site in dep-chain-check.sh: a crate called `thiserror` matches
    # `grep -i error`, so the tree is printed and the cause is lost.
    ( cd "$ws" && cargo tree -e no-dev --manifest-path "$entry" --config "$edir/nros-cargo.toml" 2>&1 1>/dev/null | head -10 | sed 's/^/      /' ) >&2
    exit 1
fi
echo "::endgroup::"

echo "scaffold-journey: PASS (${plat})"
