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
    local store="${NROS_HOME:-$HOME/.nros}/sdk"
    local prefix
    local -a ordered=()
    while IFS= read -r prefix; do
        [ -n "$prefix" ] && ordered+=("$prefix")
    done < <(
        for prefix in "$store"/corrosion/*/; do
            [ -d "$prefix" ] || continue
            printf '%s\n' "${prefix%/}"
        done | sort -Vr
    )
    ordered+=("$store/corrosion")
    for prefix in "${ordered[@]}"; do
        [ -d "$prefix" ] || continue
        # Keep only a prefix a `find_package` can actually resolve FROM.
        if compgen -G "$prefix/lib*/cmake/Corrosion/CorrosionConfig.cmake" >/dev/null ||
           compgen -G "$prefix/lib/*/cmake/Corrosion/CorrosionConfig.cmake" >/dev/null ||
           compgen -G "$prefix/share/cmake/Corrosion/CorrosionConfig.cmake" >/dev/null; then
            printf '%s\n' "$prefix"
        fi
    done
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
