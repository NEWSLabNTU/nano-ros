#!/usr/bin/env bash
# The ONE spelling of "which nano-ros checkout is this Zephyr build's `nros`
# module" — issue 1379, completing issue 1258 / phase-449 W1.
#
# ## Why a build has to say this at all
#
# A Zephyr image gets the nano-ros `nros` module from west's project list. Until
# phase-449 W1 the store workspace's manifest project was a SYMLINK to whichever
# checkout ran `just zephyr setup`, so every build in that workspace compiled
# THAT tree's `zephyr/` module, platform sources and nros-c/nros-cpp headers —
# whatever commit it happened to be at — beside the entry code the invoking
# checkout generated. Measured on this host, in the very build dir issue 1379
# was filed from:
#
#   APPLICATION_SOURCE_DIR = <worktree>/examples/zephyr/rust/listener
#   NROS_REPO_DIR          = <another clone>/third-party/nano-ros
#   zephyr_modules.txt     "nros":"<workspace>/nano-ros":"<another clone>/…/zephyr"
#
# Every cache entry the INVOKING build set named the invoking checkout; the one
# entry derived from the MODULE named a different tree. So the mixing is not a
# stale cmake cache in a shared build dir (the issue's first guess, and it does
# not survive that measurement) — it is the module binding, and it would repeat
# in a freshly created build directory.
#
# phase-449 W1 unbound the workspace and taught THREE builders to name their own
# module. There were more than three: `just zephyr build-one` — the exact
# command issue 1379 reports — was not among them, and neither were the two FVP
# recipes. That is CLAUDE.md's "fix the CLASS" rule paid for again, so the flag
# now has one spelling and a gate (`check-zephyr-module-binding`) that keeps the
# next `west build` from being added without it.
#
# ## Why the checkout ROOT and not `<checkout>/zephyr`
#
# A Zephyr module is the directory HOLDING `zephyr/module.yml`. Pointing
# `ZEPHYR_EXTRA_MODULES` at the `zephyr/` subdirectory fails configure with
# "is not a valid zephyr module" — a message that names the variable and not the
# rule, which is why it is written down here rather than at each call site.
#
# ## Usable both ways
#
# `source` it for the functions, or run it as a command (that is how a `just`
# expression reaches it):
#
#   scripts/lib/zephyr-module.sh [--root DIR] <root|cmake-arg>
#
# shellcheck shell=bash

# The checkout this file belongs to, physical. Same derivation
# `scripts/lib/zephyr-workspace.sh` makes, for the same reason: a caller that
# already holds a root passes it rather than paying for a second `cd`.
nros_zephyr_module_root() {
    local root="${1:-}"
    if [ -n "$root" ]; then
        (cd "$root" && pwd -P)
        return 0
    fi
    (cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)
}

# The single `-D` every configuring `west build` of a nano-ros image must pass.
# Printed as ONE argument, so callers quote it as one: `"$(… cmake-arg)"`.
nros_zephyr_module_cmake_arg() {
    printf '%s\n' "-DZEPHYR_EXTRA_MODULES=$(nros_zephyr_module_root "${1:-}")"
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    set -uo pipefail

    _nros_zmod_root=""
    _nros_zmod_mode=""
    while [ $# -gt 0 ]; do
        case "$1" in
            --root)
                _nros_zmod_root="${2:-}"
                shift 2
                ;;
            -h | --help)
                sed -n '2,50p' "${BASH_SOURCE[0]}"
                exit 0
                ;;
            -*)
                echo "zephyr-module.sh: unknown option $1" >&2
                exit 2
                ;;
            *)
                if [ -n "$_nros_zmod_mode" ]; then
                    echo "zephyr-module.sh: unexpected argument $1" >&2
                    exit 2
                fi
                _nros_zmod_mode="$1"
                shift
                ;;
        esac
    done

    case "${_nros_zmod_mode:-cmake-arg}" in
        root) nros_zephyr_module_root "$_nros_zmod_root" ;;
        cmake-arg) nros_zephyr_module_cmake_arg "$_nros_zmod_root" ;;
        *)
            echo "zephyr-module.sh: unknown mode ${_nros_zmod_mode}" >&2
            echo "  modes: root cmake-arg" >&2
            exit 2
            ;;
    esac
fi
