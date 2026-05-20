#!/usr/bin/env bash
#
# Phase 173.3 — drift gate between the orchestration generator's
# PlatformProfile board-crate references and the actual board crates.
#
# The generator (`profile()` + `render_platform_dependencies` in the
# colcon-nano-ros submodule) names a board crate for every BoardRun /
# host platform it supports. This gate asserts, for every
# `packages/boards/nros-board-*` path the generator references:
#
#   1. the crate directory exists under the workspace,
#   2. it has a Cargo.toml,
#   3. it exposes a board entry (`pub fn run` / `pub fn run_generic`,
#      or re-exports one), i.e. the `Board`-driven entry the generated
#      `main.rs` calls.
#
# Catches "added a profile row, forgot / renamed the board crate" and
# "moved a board crate, forgot to update the generator". Mirrors
# `scripts/check-platform-abi-mirror.sh` /
# `scripts/check-board-abi-mirror.sh`. Hooked from `just check`.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GENERATOR="$ROOT/packages/codegen/packages/nros-cli-core/src/orchestration/generate.rs"

if [[ ! -f "$GENERATOR" ]]; then
    # The generator lives in the colcon-nano-ros submodule. On a
    # checkout that hasn't run `git submodule update`, there is nothing
    # to drift against — skip rather than fail `just check`.
    echo "skip: generator not present ($GENERATOR) — colcon-nano-ros submodule not checked out"
    exit 0
fi

# Every distinct `packages/boards/nros-board-*` path literal the
# generator references. `sort -u` collapses the repeats.
mapfile -t BOARD_PATHS < <(
    grep -oE 'packages/boards/nros-board-[a-zA-Z0-9_-]+' "$GENERATOR" | sort -u
)

if (( ${#BOARD_PATHS[@]} == 0 )); then
    echo "error: no board-crate paths found in $GENERATOR" >&2
    echo "       (did render_platform_dependencies change shape?)" >&2
    exit 1
fi

fail=0
for rel in "${BOARD_PATHS[@]}"; do
    dir="$ROOT/$rel"
    if [[ ! -d "$dir" ]]; then
        echo "drift: generator references missing board crate: $rel" >&2
        fail=1
        continue
    fi
    if [[ ! -f "$dir/Cargo.toml" ]]; then
        echo "drift: $rel has no Cargo.toml" >&2
        fail=1
        continue
    fi
    # Board entry: a direct `fn run` / `fn run_generic`, or a re-export
    # of one (`pub use ...::{... run ...}` / `pub use ...run;`).
    if ! grep -rqE 'pub fn run\b|pub fn run_generic\b|pub use[^;]*\brun\b' "$dir/src" 2>/dev/null; then
        echo "drift: $rel exposes no board entry (pub fn run / run_generic / re-export)" >&2
        fail=1
        continue
    fi
    # Phase 173.3 — a crate that exposes a *concrete* board entry
    # (`pub fn run`, i.e. the board ZST itself — NOT a `run_generic<B>`
    # family base driver, and NOT the trait-defining `nros-board-common`)
    # must implement the `Board` super-trait: all three sub-traits
    # `BoardInit` + `BoardPrint` + `BoardExit`. This is the acceptance
    # requirement "drift gate fails when a PlatformProfile row lacks a
    # Board impl" — catches "added a board + its `run`, forgot the impls".
    # Base/common crates (run_generic / trait def) are exempt.
    if [[ "$rel" != *nros-board-common ]] \
       && grep -rqE 'pub fn run\b' "$dir/src" 2>/dev/null \
       && ! grep -rqE 'pub fn run_generic\b' "$dir/src" 2>/dev/null; then
        # Match the trait-impl head `… <Trait> for …` (covers both
        # `impl BoardExit for X` and `impl nros_board_common::BoardExit
        # for X`). The `<Trait> for` adjacency is the robust signal.
        for tr in BoardInit BoardPrint BoardExit; do
            if ! grep -rqE "\b${tr}[[:space:]]+for\b" "$dir/src" 2>/dev/null; then
                echo "drift: $rel has a concrete board entry but no \`${tr}\` impl (Board super-trait incomplete)" >&2
                fail=1
            fi
        done
    fi
done

if (( fail )); then
    exit 1
fi

echo "profile↔board mirror clean: ${#BOARD_PATHS[@]} board crates match generator references"
