#!/usr/bin/env bash

# Reconfigure a CMake build dir only when the source/signature inputs change.
# Usage: nros_cmake_configure_if_needed <source-dir> <build-dir> [cmake args...]
nros_cmake_configure_if_needed() {
    local src_dir="$1"
    local build_dir="$2"
    shift 2

    local abs_src_dir
    local abs_build_dir
    abs_src_dir="$(realpath "$src_dir")"
    abs_build_dir="$(realpath -m "$build_dir")"

    local sig_file="$build_dir/.nros-cmake.sig"
    mkdir -p "$build_dir"

    local desired
    desired="$(
        printf 'source=%s\n' "$abs_src_dir"
        printf 'cmake=%s\n' "$(cmake --version | head -1)"
        printf 'args=%q\n' "$@"
        find "$abs_src_dir" \
            \( -path "$abs_build_dir" -o -name 'build' -o -name 'build-*' \) -prune -o \
            \( -name CMakeLists.txt -o -name package.xml -o -path '*/src/*' -o -path '*/include/*' \) \
            -type f -print0 2>/dev/null \
            | sort -z \
            | xargs -0 sha1sum 2>/dev/null || true
    )"

    if [ ! -f "$build_dir/CMakeCache.txt" ] || \
       { [ ! -f "$build_dir/Makefile" ] && [ ! -f "$build_dir/build.ninja" ]; } || \
       [ ! -f "$sig_file" ] || \
       [ "$(cat "$sig_file")" != "$desired" ]; then
        cmake -S "$src_dir" -B "$build_dir" "$@"
        printf '%s\n' "$desired" > "$sig_file"
    fi
}
