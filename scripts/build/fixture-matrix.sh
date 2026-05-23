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
