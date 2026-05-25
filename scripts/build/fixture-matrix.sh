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
        # Record the signature ONLY after a successful configure. Writing it
        # unconditionally (the old behaviour) poisoned the build dir on a
        # failed configure: a later retry with the env fixed saw a matching
        # signature, skipped reconfigure, and ran `cmake --build` on a build
        # dir with no generated build system → "gmake: Makefile: No such
        # file". The parallel callers run each job in a `set +e` bash -c, so
        # the failure could not abort the function on its own.
        if ! cmake -S "$src_dir" -B "$build_dir" "$@"; then
            return 1
        fi
        printf '%s\n' "$signature" > "$sig_file"
    fi
    cmake --build "$build_dir"
}
