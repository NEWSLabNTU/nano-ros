#!/usr/bin/env bash

# Shared build-dir invalidation rules (toolchain file + compiler version).
# shellcheck source=scripts/build/cmake-cache-guard.sh
. "$(dirname "${BASH_SOURCE[0]}")/cmake-cache-guard.sh"
# issue 0493 — the ONE CMAKE_PREFIX_PATH derivation (SDK Corrosion). Sourced
# here so every caller of `nros_cmake_configure_if_needed` gets it without
# repeating the wiring: workspace-fixtures-build.sh, fixtures-build.sh,
# phase226-cxx-eff.sh and just/native.just all route through this function.
# shellcheck source=scripts/build/cmake-prefix.sh
. "$(dirname "${BASH_SOURCE[0]}")/cmake-prefix.sh"
# Export at SOURCE time, not inside the configure function: `fixtures-build.sh`
# runs its rows in `make` workers via `export -f`, and a function called from an
# exported function has to be exported too or the worker dies "command not
# found" (0400's cache guard is exactly that bug). An exported ENV VAR is
# inherited by every descendant with nothing to re-export.
nros_cmake_export_prefix_path

# Issue 1840 — the variables activation deliberately does NOT export (the
# index's `[source.*] env` names and the `# just-only:` checkout root,
# RFC-0103 D4/D6) are exactly the ones a build run OUTSIDE `just` may still
# inherit from a shell activated in ANOTHER checkout — W2d stopped exporting
# them, which also stopped re-rooting them. `just` re-roots every one for its
# own processes (`just/sdk-env.just`, issue 1280); this does the same for the
# processes THIS script starts, through the ONE rule (`checkout-paths.sh`):
# outside any checkout -> kept, this checkout -> kept, another -> re-rooted.
# Re-rooting (not unsetting) is right HERE and wrong in activation: these are
# this build's own children, which RFC-0103 D4 scopes out of "the user's
# shell". At SOURCE time, so `make` workers inherit the result as plain env.
_nros_cmake_incr_lib="$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)"
# shellcheck source=scripts/lib/checkout-paths.sh
. "$_nros_cmake_incr_lib/checkout-paths.sh"
# shellcheck source=scripts/lib/unexported-env-names.sh
. "$_nros_cmake_incr_lib/unexported-env-names.sh"
# The checkout this script was read from — the root an inherited value is
# re-rooted ONTO when the source dir itself belongs to no checkout. Exported
# because `nros_cmake_configure_if_needed` runs in `make` workers.
_NROS_CMAKE_INCR_HERE="$(nros_checkout_root "$(cd "$_nros_cmake_incr_lib/../.." && pwd -P)")"
export _NROS_CMAKE_INCR_HERE
nros_cmake_reroot_inherited() {
    local here="$1" n v nv
    [ -n "$here" ] || return 0
    while IFS= read -r n; do
        [ -n "$n" ] || continue
        v="$(printenv "$n" 2>/dev/null)" || continue
        [ -n "$v" ] || continue
        nv="$(nros_reroot_checkout_path "$v" "$here")"
        if [ "$nv" != "$v" ]; then
            echo "  (inherited $n named another nano-ros checkout — re-rooted onto $here; issue 1840)" >&2
            export "$n=$nv"
        fi
    done < <(nros_unexported_env_names "$here")
}
nros_cmake_reroot_inherited "$_NROS_CMAKE_INCR_HERE"
unset _nros_cmake_incr_lib

# nros_cmake_resolve_nano_ros_root <source-dir>
#
# Export the `nano_ros_ROOT` a configure of <source-dir> must use, and set
# `_nros_cmake_here` to the checkout the build belongs to (empty when none). RFC-0048 (phase-287):
# an ament-shape example resolves nano-ros through `find_package(nano_ros)`,
# which locates the in-tree nano_rosConfig.cmake via CMake's `nano_ros_ROOT`
# env var. The checkout that encloses the source dir comes FIRST (the one
# marker, RFC-0103 D6); `$NROS_REPO_DIR` only when the source is outside every
# checkout. A value that is SET is not therefore trusted (issue 1840): it goes
# through the one rule against the source's checkout, so an inherited
# `<main>/` in a worktree is re-rooted onto the worktree, while an out-of-tree
# SDK root is kept. Copy-out builds outside the tree pass
# `-Dnano_ros_ROOT=<checkout>` per the RFC-0026 contract.
nros_cmake_resolve_nano_ros_root() {
    local src_abs here value
    src_abs="$(cd "$1" 2>/dev/null && pwd -P)" || src_abs=""
    here="$(nros_checkout_root "$src_abs")"
    if [ -n "$here" ]; then
        value="${nano_ros_ROOT:-$here}"
    else
        here="${_NROS_CMAKE_INCR_HERE:-}"
        value="${nano_ros_ROOT:-${NROS_REPO_DIR:-}}"
    fi
    if [ -n "$value" ]; then
        nano_ros_ROOT="$(nros_reroot_checkout_path "$value" "$here")"
        export nano_ros_ROOT
    fi
    _nros_cmake_here="$here"
}

# Configure a CMake build dir only when needed, then leave incrementality to
# CMake + the generator (Phase 181.7b). `cmake --build` (run by the caller)
# auto-reconfigures on CMakeLists / dependency-graph changes via
# `cmake_check_build_system`, and the generator (Ninja, or Make as fallback)
# recompiles changed sources.
#
# Recipe-provided configure arguments are still part of the build identity. Keep
# an argument stamp and rerun `cmake -S/-B` when those arguments change, without
# deleting the build tree. That preserves warm C/C++ and Cyclone object state
# while still updating cache variables such as NROS_RMW, the codegen tool path,
# or CycloneDDS source-selection flags.
#
# Usage: nros_cmake_configure_if_needed <source-dir> <build-dir> [cmake args...]
nros_cmake_configure_if_needed() {
    local src_dir="$1"
    local build_dir="$2"
    shift 2

    # Prefer Ninja when available (clean incremental behaviour, fifo-jobserver
    # fit); otherwise CMake's default generator.
    local gen=()
    local want_gen="default"
    if command -v ninja >/dev/null 2>&1; then
        gen=(-G Ninja)
        want_gen="Ninja"
    fi

    # Switching generators in-place errors; wipe a dir configured with a
    # different one so it reconfigures cleanly.
    if [ -f "$build_dir/CMakeCache.txt" ]; then
        local cur_gen
        cur_gen="$(sed -n 's/^CMAKE_GENERATOR:INTERNAL=//p' "$build_dir/CMakeCache.txt")"
        if { [ "$want_gen" = "Ninja" ] && [ "$cur_gen" != "Ninja" ]; } || \
           { [ "$want_gen" = "default" ] && [ "$cur_gen" = "Ninja" ]; }; then
            rm -rf "$build_dir"
        fi
    fi

    # A cached CMakeCache pins CMAKE_C/CXX_COMPILER at FIRST configure; passing a
    # different -DCMAKE_TOOLCHAIN_FILE on a re-configure does NOT swap the
    # compiler (CMake reads the toolchain file only against a fresh cache). So a
    # dir first configured without the cross toolchain — e.g. a bare
    # `fixtures-build.sh <cross-platform> c` call that defaulted to host cc —
    # stays host-cc forever, silently feeding a RISC-V / ARM kernel to the x86
    # assembler ("Error: no such instruction: csrrci %rax,mstatus"). The
    # arg-stamp below WOULD reconfigure on the changed toolchain arg, but the
    # reconfigure can't move the compiler. Detect a toolchain-file mismatch vs
    # the cache and WIPE so the new compiler actually takes effect (issue 0391).
    nros_cmake_guard_build_dir "$build_dir" "$@"

    # Issue 1840 — resolve the root BEFORE the stamp, because it is part of the
    # build's identity: a dir configured against another checkout's runtime
    # must reconfigure when the root moves, and the argument stamp alone never
    # saw it (the root reaches cmake through the environment).
    local _nros_cmake_here=""
    nros_cmake_resolve_nano_ros_root "$src_dir"

    mkdir -p "$build_dir"

    local stamp_file="$build_dir/.nros-cmake-configure.args"
    local stamp_tmp="$build_dir/.nros-cmake-configure.args.tmp"
    {
        printf 'src=%q\n' "$src_dir"
        printf 'generator=%q\n' "$want_gen"
        printf 'nano_ros_ROOT=%q\n' "${nano_ros_ROOT:-}"
        local arg
        for arg in "$@"; do
            printf 'arg=%q\n' "$arg"
        done
    } > "$stamp_tmp"

    local needs_configure=0
    # Configure on missing cache, on a cache with no generated build system
    # (e.g. a previously-failed configure), or when recipe-level configure args
    # changed. Otherwise `cmake --build` handles dependency reconfigure.
    if [ ! -f "$build_dir/CMakeCache.txt" ] || \
       { [ ! -f "$build_dir/build.ninja" ] && [ ! -f "$build_dir/Makefile" ]; } || \
       ! cmp -s "$stamp_tmp" "$stamp_file"; then
        needs_configure=1
    fi

    if [ "$needs_configure" -eq 1 ]; then
        # Issue 1840 — a cache bound to ANOTHER checkout cannot be reconfigured
        # into this one (cmake-cache-guard.sh rule 3). Checked only on the
        # configure path: the stamp now carries the root, so every root move —
        # and every dir stamped before it did — arrives here.
        nros_cmake_guard_foreign_checkout "$build_dir" "$_nros_cmake_here"
        cmake -S "$src_dir" -B "$build_dir" "${gen[@]}" "$@"
        mv "$stamp_tmp" "$stamp_file"
    else
        rm -f "$stamp_tmp"
    fi
}
