#!/usr/bin/env bash
# Phase 196.7 — user-journey check: a `nros new` project resolves end-to-end via
# the source-release dependency convention (RFC-0040), not crates.io.
#
# Exercises exactly the documented out-of-tree flow:
#   1. `nros new <name> --platform <p> --lang rust`  (scaffold; emits `version = "*"`)
#   2. NROS_REPO_DIR=<repo> + `nros sync`          (write the [patch.crates-io] block)
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

# An embedded cell.
#
# This used to say `--platform native` "scaffolds nros commented out (a stub),
# so it would not exercise the patch block". That premise is STALE (issue 0916)
# — the Hosted template emits an ACTIVE `nros` dep, and the commented-out form
# was retired. Measured:
#
#   $ nros new probe --platform native --lang rust
#   nros = { version = "*", default-features = false, features = [...] }
#   nros-board-linux = { version = "*", default-features = false }
#
# So `native` would satisfy this check today. The choice below is NOT that
# reason; it is the one immediately after, which still holds.
#
# `baremetal`, not `freertos`: since `fix(#333)` (2026-07-28) `nros new` REFUSES
# freertos for Rust —
#
#   nros new: single-package Rust scaffolding for --platform freertos is not
#   available yet — the tracked shape is a split node-lib + `*-entry` bin pair
#
# which is deliberate (that shape has no single-package template), but this job
# was not moved with it and has failed on every run since. Any platform the CLI
# still scaffolds and that emits ACTIVE deps satisfies this check; baremetal,
# esp32 and posix all do. Verified with `nros new`: each yields an uncommented
# `nros` dep and a board dep.
#
# If freertos scaffolding lands, moving back is fine — the check is about the
# dependency convention, not about which platform carries it.
plat="baremetal"
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

echo "::group::nros sync (NROS_REPO_DIR=${REPO})"
( cd "$proj" && NROS_REPO_DIR="$REPO" "$NROS" sync )
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
