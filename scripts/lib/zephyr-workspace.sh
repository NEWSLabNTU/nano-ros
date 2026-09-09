#!/usr/bin/env bash
# The ONE Zephyr workspace resolver — RFC-0095 D4, phase-440 W1.
#
# Until this file existed the chain was spelled THREE times, and the three did
# not agree:
#
#   just/zephyr.just:19            env -> (4.4 ? ../nano-ros-workspace-4.4)
#                                      -> zephyr-workspace -> ../nano-ros-workspace
#                                      -> "zephyr-workspace" (the install target)
#                                  predicate: the DIRECTORY exists
#   scripts/build/west-fixtures.sh env -> zephyr-workspace -> ../nano-ros-workspace
#                                      -> ../nano-ros-workspace-4.4 -> ""
#                                  predicate: <cand>/zephyr exists; version-BLIND,
#                                  and a set-but-empty override fell THROUGH
#   scripts/check-tier-…sh:246     env -> zephyr-workspace -> ../nano-ros-workspace -> ""
#                                  predicate: <cand>/zephyr exists; no 4.4 arm at all
#
# Three ladders answering one question is the two-spellings shape this repo
# keeps paying for (CLAUDE.md "Fix the CLASS"), and here it was already
# observable: on the 4.4 line `just zephyr` and the west fixture builder pointed
# at different trees. Folding them has to pick ONE answer, so the reconciliation
# is stated rather than discovered:
#
#   * the OVERRIDE wins unconditionally. `$NROS_ZEPHYR_WORKSPACE` is also the
#     INSTALL TARGET of `just zephyr setup`, so it must resolve before the tree
#     it names exists. An override silently ignored because the tree is not
#     there yet is the worse failure (west-fixtures had it).
#   * a candidate is a workspace when it holds `zephyr/`. That is what
#     `ZEPHYR_BASE` points at, so "the directory exists" (just's old predicate)
#     accepted a half-finished `zephyr-workspace/` that west cannot use.
#   * the ladder is VERSION-AWARE, one line at a time. A 3.7 run never resolves
#     the 4.4 sibling and a 4.4 run never resolves the 3.7 trees — a workspace
#     of the wrong line is a west-manifest mismatch, not a fallback.
#
# ## The store arm, and why it is LAST
#
# RFC-0095 D4 ends at `$NROS_STORE/workspaces/zephyr/<version>` FIRST and the
# checkout-relative arms last. W1 adds the arm at the BOTTOM so that with no
# store populated — every host today — resolution is unchanged. W4 moves the
# trees and inverts the order. The checkout-relative arm STAYS last forever
# (RFC-0095 D4, phase-440 non-goals): a contributor patching a Zephyr module
# points `$NROS_ZEPHYR_WORKSPACE` at their own tree.
#
# The store root is `${NROS_STORE:-${NROS_HOME:-$HOME/.nros}}` — never an
# absolute literal, and never asserted as one in a gate (RFC-0095 D2), because
# a test naming an absolute path only passes on the machine it was written on.
#
# ## Two output spellings, one ladder
#
# `just` keeps the repo-RELATIVE spelling (`zephyr-workspace`,
# `../nano-ros-workspace-4.4`) because its recipes `realpath` it, echo it in
# remedies and join it into build dirs; `west-fixtures.sh` needs the ABSOLUTE
# one for `ZEPHYR_BASE`. Both are the same ladder — `_abs` joins `<root>/` onto
# what `nros_zephyr_ws_candidates` emits, with no normalisation, so the string
# is byte-identical to the `"$repo_root/…"` literals it replaces.
#
# Usable BOTH ways: `source` it for the functions, or run it as a command
# (that is how `just` reaches it — a `just` expression and a shell function
# cannot literally share code, so `just` CALLS this rather than restating it).
#
#   scripts/lib/zephyr-workspace.sh [--version V] [--root DIR] [--absolute] \
#       <candidates|resolve|resolve-or-default|default|store-dir|version>
#
# shellcheck shell=bash

# The repo root, from this file's own location (`scripts/lib/…`). Physical, the
# same derivation `west-fixtures.sh` makes for `$repo_root`, so `--absolute`
# reproduces its old literals exactly. Callers with a root already in hand pass
# it rather than paying for a second `cd`.
nros_zephyr_ws_default_root() {
    (cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
}

# The selected Zephyr line. `3.7` (LTS, west.yml) is the default the whole tree
# assumes; `4.4` is the rolling line. Same default as `just/zephyr.just`'s
# `NROS_ZEPHYR_VERSION := env("NROS_ZEPHYR_VERSION", "3.7")`.
nros_zephyr_ws_version() {
    printf '%s\n' "${NROS_ZEPHYR_VERSION:-3.7}"
}

# The store root (RFC-0095 D2). Resolved through the variables, in the order
# the tree already uses for `$NROS_HOME` — `${NROS_HOME:-$HOME/.nros}` appears
# in `scripts/build/cargo.sh`, `scripts/ci/dep-chain-check.sh` and
# `scripts/xrce-agent/build.sh` — with `NROS_STORE` layered on top as the name
# RFC-0095 gives the same directory.
nros_zephyr_ws_store_root() {
    printf '%s\n' "${NROS_STORE:-${NROS_HOME:-$HOME/.nros}}"
}

# `$NROS_STORE/workspaces/zephyr/<version>` — the arm W4 will promote.
nros_zephyr_ws_store_dir() {
    local version="${1:-}"
    [ -n "$version" ] || version="$(nros_zephyr_ws_version)"
    printf '%s\n' "$(nros_zephyr_ws_store_root)/workspaces/zephyr/$version"
}

# THE LADDER. One candidate per line, in order, in its canonical spelling —
# repo-relative for the checkout arms, absolute for the override and the store.
# Every other function here is a wrapper over this one.
#
# Args: [version] [root]
nros_zephyr_ws_candidates() {
    local version="${1:-}" root="${2:-}"
    [ -n "$version" ] || version="$(nros_zephyr_ws_version)"
    [ -n "$root" ] || root="$(nros_zephyr_ws_default_root)"

    # 1. The explicit override. It wins outright and ends the ladder: it is the
    #    install target as well as the lookup, so it may name a tree that does
    #    not exist yet.
    if [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
        printf '%s\n' "$NROS_ZEPHYR_WORKSPACE"
        return 0
    fi

    # 2. The checkout-relative trees `just zephyr setup` lands, for THIS line.
    if [ "$version" = "4.4" ]; then
        printf '%s\n' "../nano-ros-workspace-4.4"
    else
        printf '%s\n' "zephyr-workspace"
        printf '%s\n' "../nano-ros-workspace"
    fi

    # 3. The store (RFC-0095 D2). Last for now — see the header.
    nros_zephyr_ws_store_dir "$version"
}

# Where `just zephyr setup` would INSTALL this line if nothing is provisioned.
# Not a lookup: it is returned whether or not it exists, which is what makes it
# usable as a target. Args: [version]
nros_zephyr_ws_default() {
    local version="${1:-}"
    [ -n "$version" ] || version="$(nros_zephyr_ws_version)"
    if [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
        printf '%s\n' "$NROS_ZEPHYR_WORKSPACE"
        return 0
    fi
    if [ "$version" = "4.4" ]; then
        printf '%s\n' "../nano-ros-workspace-4.4"
    else
        printf '%s\n' "zephyr-workspace"
    fi
}

# The first candidate that IS a workspace, in its canonical spelling.
# Prints nothing and returns 1 when none is — callers under `set -e` want
# `x="$(nros_zephyr_ws_resolve || true)"`. Args: [version] [root]
nros_zephyr_ws_resolve() {
    local version="${1:-}" root="${2:-}" cand abs
    [ -n "$version" ] || version="$(nros_zephyr_ws_version)"
    [ -n "$root" ] || root="$(nros_zephyr_ws_default_root)"

    # The override is not existence-tested — see candidate 1.
    if [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
        printf '%s\n' "$NROS_ZEPHYR_WORKSPACE"
        return 0
    fi

    while IFS= read -r cand; do
        [ -n "$cand" ] || continue
        case "$cand" in
            /*) abs="$cand" ;;
            *) abs="$root/$cand" ;;
        esac
        if [ -d "$abs/zephyr" ]; then
            printf '%s\n' "$cand"
            return 0
        fi
    done < <(nros_zephyr_ws_candidates "$version" "$root")
    return 1
}

# `nros_zephyr_ws_resolve`, joined onto the root. No normalisation: the point is
# that `<root>/../nano-ros-workspace` stays the string it was.
# Args: [version] [root]
nros_zephyr_ws_resolve_abs() {
    local version="${1:-}" root="${2:-}" rel
    [ -n "$root" ] || root="$(nros_zephyr_ws_default_root)"
    rel="$(nros_zephyr_ws_resolve "$version" "$root")" || return 1
    case "$rel" in
        /*) printf '%s\n' "$rel" ;;
        *) printf '%s\n' "$root/$rel" ;;
    esac
}

# The resolved workspace, or the install target when nothing is provisioned.
# This is `just`'s arm: a `just zephyr setup` needs a path either way.
# Args: [version] [root]
nros_zephyr_ws_resolve_or_default() {
    local version="${1:-}" root="${2:-}"
    nros_zephyr_ws_resolve "$version" "$root" && return 0
    nros_zephyr_ws_default "$version"
}

# ---------------------------------------------------------------------------
# Command form. `just` cannot call a shell function, so it calls this.
# ---------------------------------------------------------------------------
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    set -uo pipefail

    _nros_zws_version=""
    _nros_zws_root=""
    _nros_zws_absolute=0
    _nros_zws_mode=""

    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                _nros_zws_version="${2:-}"
                shift 2
                ;;
            --root)
                _nros_zws_root="${2:-}"
                shift 2
                ;;
            --absolute)
                _nros_zws_absolute=1
                shift
                ;;
            -h | --help)
                sed -n '2,60p' "${BASH_SOURCE[0]}"
                exit 0
                ;;
            -*)
                echo "zephyr-workspace.sh: unknown option $1" >&2
                exit 2
                ;;
            *)
                if [ -n "$_nros_zws_mode" ]; then
                    echo "zephyr-workspace.sh: unexpected argument $1" >&2
                    exit 2
                fi
                _nros_zws_mode="$1"
                shift
                ;;
        esac
    done

    [ -n "$_nros_zws_root" ] || _nros_zws_root="$(nros_zephyr_ws_default_root)"

    case "${_nros_zws_mode:-resolve-or-default}" in
        candidates)
            if [ "$_nros_zws_absolute" -eq 1 ]; then
                while IFS= read -r _nros_zws_cand; do
                    case "$_nros_zws_cand" in
                        /*) printf '%s\n' "$_nros_zws_cand" ;;
                        *) printf '%s\n' "$_nros_zws_root/$_nros_zws_cand" ;;
                    esac
                done < <(nros_zephyr_ws_candidates "$_nros_zws_version" "$_nros_zws_root")
            else
                nros_zephyr_ws_candidates "$_nros_zws_version" "$_nros_zws_root"
            fi
            ;;
        resolve)
            if [ "$_nros_zws_absolute" -eq 1 ]; then
                nros_zephyr_ws_resolve_abs "$_nros_zws_version" "$_nros_zws_root"
            else
                nros_zephyr_ws_resolve "$_nros_zws_version" "$_nros_zws_root"
            fi
            ;;
        resolve-or-default)
            # `--absolute` is deliberately not offered here: the default arm is
            # a path that does not exist yet, and joining a root onto it would
            # hand `just` an absolute string where every recipe expects the
            # relative one it prints in its own remedies.
            nros_zephyr_ws_resolve_or_default "$_nros_zws_version" "$_nros_zws_root"
            ;;
        default)
            nros_zephyr_ws_default "$_nros_zws_version"
            ;;
        store-dir)
            nros_zephyr_ws_store_dir "$_nros_zws_version"
            ;;
        version)
            if [ -n "$_nros_zws_version" ]; then
                printf '%s\n' "$_nros_zws_version"
            else
                nros_zephyr_ws_version
            fi
            ;;
        *)
            echo "zephyr-workspace.sh: unknown mode ${_nros_zws_mode}" >&2
            echo "  modes: candidates resolve resolve-or-default default store-dir version" >&2
            exit 2
            ;;
    esac
fi
