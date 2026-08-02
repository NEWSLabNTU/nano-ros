#!/usr/bin/env bash

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
    if [ -f "$build_dir/CMakeCache.txt" ]; then
        local want_tc="" cached_tc _a
        for _a in "$@"; do
            case "$_a" in
                -DCMAKE_TOOLCHAIN_FILE=*) want_tc="${_a#-DCMAKE_TOOLCHAIN_FILE=}" ;;
            esac
        done
        cached_tc="$(sed -n 's/^CMAKE_TOOLCHAIN_FILE:[^=]*=//p' "$build_dir/CMakeCache.txt")"
        if [ "$want_tc" != "$cached_tc" ]; then
            echo "  (toolchain change: '${cached_tc:-<none>}' -> '${want_tc:-<none>}' — wiping $build_dir)" >&2
            rm -rf "$build_dir"
        fi
    fi

    mkdir -p "$build_dir"

    local stamp_file="$build_dir/.nros-cmake-configure.args"
    local stamp_tmp="$build_dir/.nros-cmake-configure.args.tmp"
    {
        printf 'src=%q\n' "$src_dir"
        printf 'generator=%q\n' "$want_gen"
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
        # RFC-0048 (phase-287): an ament-shape example resolves nano-ros through
        # `find_package(nano_ros)`, which locates the in-tree nano_rosConfig.cmake
        # via CMake's `nano_ros_ROOT` env var. A fixture / CI build that did not
        # `source ./activate.sh` won't have it — derive it here (from NROS_REPO_DIR
        # / NANO_ROS_ROOT, else a walk-up to the `nros-sdk-index.toml` sentinel)
        # so every in-tree build path resolves it. Copy-out builds outside the
        # tree pass `-Dnano_ros_ROOT=<checkout>` per the RFC-0026 contract.
        if [ -z "${nano_ros_ROOT:-}" ]; then
            local _nrr="${NROS_REPO_DIR:-${NANO_ROS_ROOT:-}}"
            if [ -z "$_nrr" ]; then
                local _d
                _d="$(cd "$src_dir" && pwd)"
                while [ -n "$_d" ] && [ "$_d" != "/" ] && [ ! -f "$_d/nros-sdk-index.toml" ]; do
                    _d="$(dirname "$_d")"
                done
                [ -f "$_d/nros-sdk-index.toml" ] && _nrr="$_d"
            fi
            [ -n "$_nrr" ] && export nano_ros_ROOT="$_nrr"
        fi
        cmake -S "$src_dir" -B "$build_dir" "${gen[@]}" "$@"
        mv "$stamp_tmp" "$stamp_file"
    else
        rm -f "$stamp_tmp"
    fi
}
