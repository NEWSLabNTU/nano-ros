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
        cmake -S "$src_dir" -B "$build_dir" "${gen[@]}" "$@"
        mv "$stamp_tmp" "$stamp_file"
    else
        rm -f "$stamp_tmp"
    fi
}
