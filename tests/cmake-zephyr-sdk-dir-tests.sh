#!/bin/bash
# tests/cmake-zephyr-sdk-dir-tests.sh -- issue 3004
#
# `nros_zephyr_sdk_dir_check` (zephyr/cmake/nros_zephyr_sdk_dir.cmake) runs in
# the module hook before Zephyr hands ZEPHYR_SDK_INSTALL_DIR to find_package.
# A fake store laid out both ways -- `<v>/zephyr-sdk-<v>/sdk_version` -- and
# the cases it must answer:
#   the root itself                    left alone
#   the store's `<v>/` (one level up)  resolved to the root, with a status line
#   one level up, another version      the one root below it
#   two levels up (several versions)   FATAL, naming the remedy
#   a path that does not exist         FATAL
# Before the check, every one of these reached find_package, which falls back
# to the CMake registry without a word.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail=0

mk_sdk() { mkdir -p "$1/cmake"; printf '%s\n' "$2" >"$1/sdk_version"; }
store="$tmp/sdk/zephyr-sdk"
mk_sdk "$store/0.16.9/zephyr-sdk-0.16.9" 0.16.9
mk_sdk "$store/0.16.8/zephyr-sdk-0.16.8" 0.16.8
mkdir -p "$tmp/zephyr"
echo 0.16.9 >"$tmp/zephyr/SDK_VERSION"
printf '%s\n' \
    "set(ZEPHYR_BASE \"$tmp/zephyr\")" \
    "include(\"$root/zephyr/cmake/nros_zephyr_sdk_dir.cmake\")" \
    "nros_zephyr_sdk_dir_check()" \
    "message(STATUS \"RESULT=\$ENV{ZEPHYR_SDK_INSTALL_DIR}\")" \
    >"$tmp/drive.cmake"

run() { # <ZEPHYR_SDK_INSTALL_DIR> [pin] -> rc, out
    [ -n "${2:-}" ] && echo "$2" >"$tmp/zephyr/SDK_VERSION"
    rc=0
    out="$(ZEPHYR_SDK_INSTALL_DIR="$1" cmake -P "$tmp/drive.cmake" 2>&1)" || rc=$?
    echo 0.16.9 >"$tmp/zephyr/SDK_VERSION"
    # CMake wraps a message across lines; match on one line.
    flat="$(tr '\n' ' ' <<<"$out" | tr -s ' ')"
}
check() { # <what> <0 = holds>
    if [ "$2" = 0 ]; then echo "  ok   $1"; else echo "  FAIL $1"; echo "$out" | sed 's/^/       /'; fail=1; fi
}

run "$store/0.16.9/zephyr-sdk-0.16.9"
check "the SDK root is left alone" \
    "$([ "$rc" -eq 0 ] && grep -q "RESULT=$store/0.16.9/zephyr-sdk-0.16.9\$" <<<"$out" \
        && ! grep -q 'directory above' <<<"$out"; echo $?)"

run "$store/0.16.9"
check "the store's version directory resolves to its root, and says so" \
    "$([ "$rc" -eq 0 ] && grep -q "RESULT=$store/0.16.9/zephyr-sdk-0.16.9\$" <<<"$out" \
        && grep -q 'directory above the SDK root' <<<"$flat"; echo $?)"

run "$store/0.16.8/" 0.16.9
check "one level up at another version names the one root below it" \
    "$([ "$rc" -eq 0 ] && grep -q "RESULT=$store/0.16.8/zephyr-sdk-0.16.8\$" <<<"$out"; echo $?)"

run "$store"
check "two levels up stops the configure, naming the remedy" \
    "$([ "$rc" -ne 0 ] && grep -q 'names no Zephyr SDK' <<<"$flat" \
        && grep -q 'nros locate tool zephyr-sdk' <<<"$flat"; echo $?)"

run "$tmp/nowhere"
check "a path that does not exist stops the configure" \
    "$([ "$rc" -ne 0 ] && grep -q 'it does not exist' <<<"$flat"; echo $?)"

[ "$fail" -eq 0 ] || exit 1
echo "cmake-zephyr-sdk-dir: OK"
