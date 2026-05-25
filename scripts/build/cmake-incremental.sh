#!/usr/bin/env bash

# Configure a CMake build dir once, then leave incrementality to CMake + the
# generator (Phase 181.7b). `cmake --build` (run by the caller) auto-reconfigures
# on CMakeLists / dependency-graph changes via `cmake_check_build_system`, and
# the generator (Ninja, or Make as fallback) recompiles changed sources — so no
# custom content-hash signature is needed (it tracked src/include content that
# the generator already handles, and never tracked msg/srv/action anyway).
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

    # Configure once: missing cache, or a cache with no generated build system
    # (e.g. a previously-failed configure). Otherwise `cmake --build` handles
    # any needed reconfigure.
    if [ ! -f "$build_dir/CMakeCache.txt" ] || \
       { [ ! -f "$build_dir/build.ninja" ] && [ ! -f "$build_dir/Makefile" ]; }; then
        cmake -S "$src_dir" -B "$build_dir" "${gen[@]}" "$@"
    fi
}
