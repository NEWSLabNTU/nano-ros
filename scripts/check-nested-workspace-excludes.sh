#!/usr/bin/env bash
#
# A nested workspace's `exclude` needs a MATCHING repo-root exclude.
#
# # What breaks without it
#
# The west-built Zephyr entry leaves (`examples/workspaces/*/src/*_entry`) are
# real cargo packages that carry NO `[workspace]` table of their own, so the
# nested workspace's `nros::main!` pkg-index still resolves their sibling node
# packages. Their nested workspace `exclude`s them (plain `cargo build` must not
# try to link an RTOS app) — and that exclusion is exactly what makes cargo's
# walker keep going UP, to the repo root. If the root does not also exclude the
# path, any cargo invocation INSIDE the leaf dies with
#
#   error: current package believes it's in a workspace when it's not
#
# Both excludes are load-bearing, and the second one is invisible from the leaf
# — nothing near it says the repo root has to know about it.
#
# # Why a gate
#
# phase-331 renamed and folded workspaces; the root's five `ws-*` exclude paths
# went stale in the move and two live leaves (`realtime-rust/src/zephyr_entry`,
# `safety/src/zephyr_rust_safety_entry`) lost their root exclude. Nothing failed:
# `cargo metadata` at the ROOT is happy, `just check fast` is happy, and the
# breakage only appears when someone runs cargo (or west) from inside the leaf —
# which in CI is the embedded lane, a day of latency away.
#
# This is the CLAUDE.md fallout class named verbatim: "West-built zephyr entry
# leaves need BOTH the nested workspace `exclude` AND a repo-root `Cargo.toml`
# exclude". It has now been paid for twice, so it gets a gate rather than a
# third mention.
#
# The check is pure text (no cargo invocation), so it belongs in `check-fast`.
# Its expensive equivalent — `cargo metadata` inside every leaf — takes minutes
# and would land in a lane nobody runs per-task.

set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — the two conditionals below decide, from a grep STATUS, whether a
# leaf is unprotected by the repo-root exclude list. A grep that failed to start
# reads as "not excluded" and the gate names a specific leaf and a specific
# manifest that are both correct. `nros_grep_q` exits 2 on a tool failure.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

root_manifest="Cargo.toml"
fail=0

# Each nested workspace under examples/workspaces/ that has an `exclude`.
while IFS= read -r ws_manifest; do
    ws_dir="$(dirname "$ws_manifest")"

    # Pull the `exclude = [...]` array. Deliberately simple: one entry per
    # line is the style every manifest here uses, and a parser that accepted
    # more shapes than the repo writes would be a second spelling to keep
    # honest.
    in_exclude=0
    while IFS= read -r line; do
        case "$line" in
            exclude*=*\[*\]*)
                # Single-line form: exclude = ["a", "b"]
                entries="$(printf '%s\n' "$line" | grep -o '"[^"]*"' | tr -d '"')"
                ;;
            exclude*=*\[*) in_exclude=1; continue ;;
            *\]*) if [ "$in_exclude" = 1 ]; then in_exclude=0; fi; continue ;;
            *)
                if [ "$in_exclude" = 1 ]; then
                    entries="$(printf '%s\n' "$line" | grep -o '"[^"]*"' | tr -d '"' || true)"
                else
                    continue
                fi
                ;;
        esac

        for entry in $entries; do
            [ -n "$entry" ] || continue
            leaf="$ws_dir/$entry"
            # Only packages matter — a declarative dir (no Cargo.toml) is
            # never walked by cargo and needs no root exclude.
            [ -f "$leaf/Cargo.toml" ] || continue
            # A leaf with its OWN [workspace] table is its own root; the
            # walker stops there and the repo root never sees it.
            if nros_grep_q '^\[workspace\]' "$leaf/Cargo.toml"; then continue; fi
            if ! nros_grep_q "\"$leaf\"" "$root_manifest"; then
                echo "[FAIL] $leaf" >&2
                echo "       excluded by $ws_manifest but NOT by the repo-root Cargo.toml." >&2
                echo "       cargo run from inside it will fail: \"current package believes" >&2
                echo "       it's in a workspace when it's not\"." >&2
                fail=1
            fi
        done
    done <"$ws_manifest"
done < <(git ls-files 'examples/workspaces/*/Cargo.toml')

# The other direction: a root exclude naming a path that no longer exists is
# how this broke — the rename left five of them behind and they read as
# coverage. Stale entries are a failure, not a wart.
while IFS= read -r path; do
    [ -e "$path" ] && continue
    echo "[FAIL] repo-root Cargo.toml excludes '$path', which does not exist." >&2
    echo "       A rename left it behind; the leaf it used to name is either gone" >&2
    echo "       or now unprotected under its new path." >&2
    fail=1
done < <(grep -o '"examples/workspaces/[^"]*"' "$root_manifest" | tr -d '"' | sort -u)

if [ "$fail" != 0 ]; then
    exit 1
fi

echo "nested-workspace excludes OK — every excluded package is excluded at the repo root too."
