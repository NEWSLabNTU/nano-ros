#!/usr/bin/env bash
# Phase 196.7 — user-journey check: a `nros new` project resolves end-to-end via
# the source-release dependency convention (RFC-0040), not crates.io.
#
# Exercises exactly the documented out-of-tree flow:
#   1. `nros new <name> --platform <p> --lang rust`  (scaffold; emits `version = "*"`)
#   2. NROS_REPO_DIR=<repo> + `nros ws sync`          (write the [patch.crates-io] block)
#   3. `cargo tree`                                    (resolution proves the patch redirects)
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
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# An embedded cell — `--platform native` scaffolds nros commented out (a stub),
# so it would not exercise the patch block; freertos emits active `nros` + board
# deps, the real convention surface.
plat="freertos"
name="uj_demo"

echo "::group::nros new ${name} --platform ${plat}"
( cd "$scratch" && "$NROS" new "$name" --platform "$plat" --lang rust )
proj="$scratch/$name"
echo "::endgroup::"

# Guard: no stale crates.io `version = "0.1"` nros/board dep leaked into the scaffold.
if grep -nE '(nros|nros-board[a-z0-9-]*)\s*=\s*\{[^}]*version\s*=\s*"0\.1"' "$proj/Cargo.toml"; then
    echo "FAIL: scaffold emitted a crates.io version = \"0.1\" dep (RFC-0040 violation)" >&2
    exit 1
fi

echo "::group::nros ws sync (NROS_REPO_DIR=${REPO})"
( cd "$proj" && NROS_REPO_DIR="$REPO" "$NROS" ws sync )
echo "::endgroup::"

# The managed patch block must redirect both `nros` and the board crate to paths.
for crate in "nros = {" "nros-board"; do
    if ! grep -q "$crate" "$proj/Cargo.toml"; then
        echo "FAIL: '$crate' not patched into the [patch.crates-io] block after sync" >&2
        sed -n '/BEGIN nros-managed/,/END nros-managed/p' "$proj/Cargo.toml" >&2
        exit 1
    fi
done

echo "::group::cargo tree (resolution)"
if ( cd "$proj" && cargo tree -e no-dev >/dev/null 2>&1 ); then
    echo "  [ok] scaffolded project resolves via the source-release patch block"
else
    echo "FAIL: cargo tree did not resolve the scaffolded project:" >&2
    ( cd "$proj" && cargo tree -e no-dev 2>&1 | grep -iE 'error|failed' | head -5 | sed 's/^/      /' ) >&2
    exit 1
fi
echo "::endgroup::"

echo "scaffold-journey: PASS (${plat})"
