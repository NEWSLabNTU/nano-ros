#!/usr/bin/env bash

# Shared build-dir invalidation rules (toolchain file + compiler version).
# shellcheck source=scripts/build/cmake-cache-guard.sh
. "$(dirname "${BASH_SOURCE[0]}")/cmake-cache-guard.sh"
# issue 0493 — the ONE CMAKE_PREFIX_PATH derivation (SDK Corrosion). This file
# carries its own `cmake -S` (`nros_cmake_fixture_build`), so it needs the
# helper directly rather than through `cmake-incremental.sh`.
# shellcheck source=scripts/build/cmake-prefix.sh
. "$(dirname "${BASH_SOURCE[0]}")/cmake-prefix.sh"
nros_cmake_export_prefix_path
# Shared fixture matrix primitives. Keep this file shell-only so every
# platform just recipe can source it without pulling in Python or Rust.

# phase-350 W1 — `nros_fixture_roles`, `nros_fixture_langs` and
# `nros_zephyr_lang_tag` lived here and were DELETED. They were the zephyr west
# lane's copy of a matrix `examples/fixtures.toml` owns (issue 0535): the leaves
# they enumerated had no `row_coord()`, so no lane could select a coordinate
# inside the zephyr module. `zephyr-fixture-leaves.sh` reads
# `fixtures-manifest.py west-leaves` now, and was their only caller.
#
# The `rust`/`c`/`cpp` -> `rs`/`c`/`cpp` tag survives as
# `fixtures-manifest.py::west_lang_tag`, one producer, until issue 0539 retires
# the short spelling of the lang axis outright.

nros_cmake_fixture_build() {
    local src_dir="$1"
    local build_dir="$2"
    # $3 (the old identity signature) is accepted for caller compatibility. The
    # active build identity is the actual configure-argument stamp below:
    # changed recipe args trigger a CMake reconfigure, not a build-dir wipe.
    shift 3

    # Prefer Ninja when available; fall back to CMake's default generator.
    local gen=()
    local want_gen="default"
    if command -v ninja >/dev/null 2>&1; then
        gen=(-G Ninja)
        want_gen="Ninja"
    fi

    # Wipe a dir configured with a different generator so the switch reconfigures.
    if [ -f "$build_dir/CMakeCache.txt" ]; then
        local cur_gen
        cur_gen="$(sed -n 's/^CMAKE_GENERATOR:INTERNAL=//p' "$build_dir/CMakeCache.txt")"
        if { [ "$want_gen" = "Ninja" ] && [ "$cur_gen" != "Ninja" ]; } || \
           { [ "$want_gen" = "default" ] && [ "$cur_gen" = "Ninja" ]; }; then
            rm -rf "$build_dir"
        fi
    fi

    # CMake pins the compiler at first configure; a later -DCMAKE_TOOLCHAIN_FILE
    # does not swap it (see cmake-incremental.sh for the full rationale — issue
    # 0391). Wipe on a toolchain-file mismatch so a host-cc-poisoned dir picks up
    # the cross compiler instead of feeding RISC-V/ARM to the x86 assembler.
    nros_cmake_guard_build_dir "$build_dir" "$@"

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

    # Configure on missing cache, on a cache with no generated build system
    # (a previously-failed configure), or when recipe-level configure args
    # changed. `cmake --build` then handles dependency reconfigure.
    local needs_configure=0
    if [ ! -f "$build_dir/CMakeCache.txt" ] || \
       { [ ! -f "$build_dir/build.ninja" ] && [ ! -f "$build_dir/Makefile" ]; } || \
       ! cmp -s "$stamp_tmp" "$stamp_file"; then
        needs_configure=1
    fi

    if [ "$needs_configure" -eq 1 ]; then
        if ! cmake -S "$src_dir" -B "$build_dir" "${gen[@]}" "$@"; then
            return 1
        fi
        mv "$stamp_tmp" "$stamp_file"
    else
        rm -f "$stamp_tmp"
    fi
    cmake --build "$build_dir"
}
