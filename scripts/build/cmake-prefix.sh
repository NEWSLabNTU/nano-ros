#!/usr/bin/env bash
# cmake-prefix.sh — the ONE shell derivation of `CMAKE_PREFIX_PATH` for a
# nano-ros cmake configure.  Source it; call `nros_cmake_export_prefix_path`
# before any `cmake -S`.
#
# issue 0493, second half. The SDK-Corrosion prefix used to be wired in exactly
# ONE of three builders:
#
#   scripts/build/compile-check-fixtures.sh    3 refs to ~/.nros/sdk/corrosion
#   scripts/build/workspace-fixtures-build.sh  0
#   scripts/build/fixtures-build.sh            0
#
# so on ONE host with ONE install, compile-check trees resolved the SDK
# Corrosion while workspace/fixture trees fell through to FetchContent — two
# different cargo target-dir topologies from one repo, which is why issue 0493
# and phase-340/344 measured contradictory trees and BOTH were right. This is
# the repo's recurring defect shape: one caller wires a rule and its siblings do
# not (the sizes-header mirror chain, #282's guard, #328's resolver,
# `fixtures-build.sh`'s `--lang` proxy). Hence one helper, and
# `check-cmake-corrosion-prefix` to keep a fourth builder from skipping it.
#
# The cmake-side authority is `cmake/NanoRosCorrosion.cmake`
# (`nros_resolve_corrosion()`), which every configure that includes this
# checkout's root reaches on its own — including a USER's, who never runs these
# scripts. This helper is the belt for the configures that do NOT: a standalone
# template calling `find_package(Corrosion)` before (or without) importing
# nano-ros. Keep the two candidate rules in step.
#
# Both install LAYOUTS are covered, because the two provisioning paths disagree:
#
#   just workspace install-corrosion   ->  $NROS_HOME/sdk/corrosion/          (flat)
#   nros setup --tool corrosion        ->  $NROS_HOME/sdk/corrosion/<version>/
#
# cmake's config search walks `<prefix>/(lib*|share)/cmake/<name>*/`, so the
# prefix is the directory CONTAINING `lib/`, never `lib/` itself. Measured on a
# host with the flat layout: prefix `…/corrosion` resolves; `…/corrosion/lib`
# and `…/corrosion/share` do not.

# Echo the SDK prefixes (newline-separated, NEWEST VERSION first, flat layout
# last). Empty when nothing is provisioned — an unprovisioned host is a
# supported state; the configure then reports a FetchContent fallback.
#
# The ordering is load-bearing and must match `_nros_corrosion_prefixes` in
# `cmake/NanoRosCorrosion.cmake` (issue 0500): the store ACCUMULATES, and
# `find_package` takes the first prefix that resolves, so plain glob order lets
# a months-old `0.5.1-nros1` shadow the `0.6.1-nros1` a provisioning run just
# wrote — silently, with the install reporting success. `sort -V` is the shell
# spelling of cmake's `COMPARE NATURAL ORDER DESCENDING`.
nros_cmake_corrosion_prefixes() {
    # phase-365 W3b — CONSTRUCT the prefix; do not enumerate the store.
    #
    # This used to glob `$store/corrosion/*/` and `sort -Vr`. Two things were
    # wrong with that, and only the second was suspected:
    #
    #   1. the glob also matched the LEGACY UNVERSIONED install's `lib/` and
    #      `share/` subdirectories, which are not versions at all — and under
    #      `sort -Vr` a pure-alpha name sorts BEFORE the numeric ones, so the
    #      0.5.x flat install led the list;
    #   2. "newest in the store" is a global answer to a per-project question:
    #      the store is shared, the pin is per-project, so a newer checkout's
    #      provisioning silently re-pointed an older one.
    #
    # `nros sdk-path corrosion` joins the store root to the version THIS project
    # pins, using the same function `nros setup` used to write the directory. No
    # ordering rule is needed because nothing is being ordered (issue 0625).
    #
    # Silence on any failure — no CLI on PATH, tool not pinned, not provisioned
    # — leaves CMAKE_PREFIX_PATH untouched, and `nros_resolve_corrosion()` then
    # reports and falls back. A wrong prefix would be worse than none.
    local dir
    dir="$(nros sdk-path corrosion 2>/dev/null)" || return 0
    [ -n "$dir" ] && [ -d "$dir" ] && printf '%s\n' "$dir"
}

# Prepend those prefixes to CMAKE_PREFIX_PATH and export it. Idempotent: a
# prefix already on the path is not added twice, so a builder that calls this
# per fixture row does not grow the variable without bound.
nros_cmake_export_prefix_path() {
    local current="${CMAKE_PREFIX_PATH:-}"
    local prefix
    while IFS= read -r prefix; do
        [ -n "$prefix" ] || continue
        case ":$current:" in
            *":$prefix:"*) continue ;;
        esac
        current="$prefix${current:+:$current}"
    done < <(nros_cmake_corrosion_prefixes)
    if [ -n "$current" ]; then
        export CMAKE_PREFIX_PATH="$current"
    fi
}
