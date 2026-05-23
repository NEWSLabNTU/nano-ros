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
    local signature="$3"
    shift 3

    local sig_file="$build_dir/.nros-cmake-fixture.sig"
    local needs_configure=0
    if [ ! -f "$build_dir/CMakeCache.txt" ] || [ ! -f "$sig_file" ]; then
        needs_configure=1
    elif [ "$(cat "$sig_file")" != "$signature" ]; then
        needs_configure=1
    fi

    if [ "$needs_configure" = "1" ]; then
        rm -rf "$build_dir"
        cmake -S "$src_dir" -B "$build_dir" "$@"
        mkdir -p "$build_dir"
        printf '%s\n' "$signature" > "$sig_file"
    fi
    cmake --build "$build_dir"
}
