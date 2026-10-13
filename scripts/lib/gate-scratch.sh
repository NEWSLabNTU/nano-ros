#!/usr/bin/env bash
# phase-489 W5 — a gate's scratch directory at a STABLE path, so sccache can hit.
#
# WHY NOT `mktemp -d`
#
# sccache's Rust key contains the cargo target dir's path (`-L dependency=<dir>`
# and the `--extern` rmeta names under it). A gate that builds under
# `mktemp -d` therefore presents a new key on every run, and a restored cache
# can never answer it. Measured on the `check-fast` gate set (sccache 0.17.0,
# a fresh checkout, two runs on the same cache, targets wiped between):
# 148 of the 208 misses that survived a same-path rerun were `api-parity`'s
# `rustdoc-ours` build under a Python tempdir, and 57 more were
# `probe-workspace-caps`' `build-uncapped` under `mktemp -d`.
# `SCCACHE_BASEDIRS` was tried first and does NOT normalise a Rust key: a
# two-crate cargo project built under two sibling target dirs missed 2/2 both
# with and without it (sccache 0.17.0).
#
# WHAT THIS GUARANTEES
#
#   * the path is a function of (checkout, name) — identical on every run of the
#     same checkout, so a CI job (whose checkout path is fixed) presents the
#     same key each time;
#   * the directory is EMPTY when handed out: the gate still builds from
#     nothing, exactly as it did under a fresh `mktemp -d`. Only the NAME
#     is stable, never the contents, so no gate can pass on a leftover.
#   * it lives under `<checkout>/build/` (gitignored), never `/tmp`: two
#     checkouts on one machine get two paths and never collide.
#
# WHAT IT DOES NOT DO: serialise. Two concurrent runs of the SAME gate in the
# SAME checkout would share a directory. `just check` runs each gate once.
#
# Usage:  dir="$(nros_gate_scratch <name>)"      # name: [A-Za-z0-9._-]+
# Python twin: scripts/lib/gate_scratch.py (`gate_scratch(name)`).

nros_gate_scratch() {
    local name="${1:-}"
    case "$name" in
        "" | */* | .*) echo "nros_gate_scratch: bad name '${name}'" >&2; return 2 ;;
    esac
    local root="${NROS_GATE_SCRATCH_ROOT:-}"
    if [ -z "$root" ]; then
        root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)/build/gate-scratch"
    fi
    local dir="$root/$name"
    rm -rf "$dir" || return 1
    mkdir -p "$dir" || return 1
    printf '%s\n' "$dir"
}
