#!/usr/bin/env bash
# Shared fixture matrix primitives. Keep this file shell-only so every
# platform just recipe can source it without pulling in Python or Rust.

nros_fixture_roles() {
    printf '%s\n' \
        talker \
        listener \
        service-server \
        service-client \
        action-server \
        action-client
}

nros_fixture_langs() {
    printf '%s\n' rust c cpp
}

nros_zephyr_lang_tag() {
    case "$1" in
        rust) printf '%s\n' rs ;;
        c) printf '%s\n' c ;;
        cpp) printf '%s\n' cpp ;;
        *)
            echo "unknown fixture language: $1" >&2
            return 2
            ;;
    esac
}

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
