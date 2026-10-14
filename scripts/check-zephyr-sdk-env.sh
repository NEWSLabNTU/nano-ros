#!/usr/bin/env bash
# Issue 1811 -- `nros_zephyr_sdk_env` (scripts/build/zephyr-toolchain.sh) names
# the SDK a Zephyr tree pins, or REFUSES an SDK board when that SDK is missing.
#
# The refusal is the point. The first version printed nothing when the pinned
# SDK was absent, and an empty answer looked the same as success. The live-peer
# board job skipped the SDK install, so its `mps2_an385` build took the CI
# image's 0.17.4 from the CMake registry and died in Zephyr's `libc-hooks.c`.
# That was the run meant to prove the fix (38291960490).
#
# Hermetic: a throwaway HOME (the CMake registry lives in it), fake SDK roots
# and a version no real install carries, so neither the store nor the in-tree
# SDK can answer for it. Then a source check: every caller tests the helper's
# status, because a refusal swallowed by a bare `$(…)` assignment is the
# silent fall-through again, one frame up.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
source "$repo_root/scripts/lib/grep-q.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fails=0
fail() { echo "check-zephyr-sdk-env: FAIL -- $*" >&2; fails=$((fails + 1)); }

# The helper's behaviour, against throwaway SDKs. Run on every invocation --
# it is the gate's negative control, not an optional extra.
selftest() {
    want=9.9.1
    other=9.9.2
    mk_sdk() { # <root> <version>
        mkdir -p "$1/cmake"
        printf '%s\n' "$2" > "$1/sdk_version"
    }
    mk_sdk "$tmp/opt/zephyr-sdk-$want" "$want"
    mk_sdk "$tmp/opt/zephyr-sdk-$other" "$other"
    mkdir -p "$tmp/tree"
    printf '%s\n' "$want" > "$tmp/tree/SDK_VERSION"

    # Registers <root> in <home>'s CMake package registry, as `setup.sh -c` does.
    register() { # <home> <root>
        mkdir -p "$1/.cmake/packages/Zephyr-sdk"
        printf '%s/cmake\n' "$2" > "$1/.cmake/packages/Zephyr-sdk/$(basename "$2")"
    }
    register "$tmp/home-both" "$tmp/opt/zephyr-sdk-$other"
    register "$tmp/home-both" "$tmp/opt/zephyr-sdk-$want"
    register "$tmp/home-other" "$tmp/opt/zephyr-sdk-$other"

    # Runs the helper in a clean bash: <home> <board> [VAR=value ...]. Prints
    # `rc=<n> out=<stdout>`, with stderr to "$tmp/err".
    probe() {
        local home="$1" board="$2"
        shift 2
        local rc=0 out
        out="$(env -u ZEPHYR_SDK_INSTALL_DIR -u ZEPHYR_TOOLCHAIN_VARIANT HOME="$home" "$@" \
            bash -c 'source "$1/scripts/build/zephyr-toolchain.sh"; nros_zephyr_sdk_env "$2" "$3"' \
            _ "$repo_root" "$board" "$tmp/tree" 2>"$tmp/err")" || rc=$?
        printf 'rc=%s out=%s' "$rc" "$out"
    }
    expect() { # <label> <got> <want>
        [ "$2" = "$3" ] || fail "$1: got \`$2\`, want \`$3\`"
    }

    expect "registered at the pinned version, beside a newer one" \
        "$(probe "$tmp/home-both" mps2_an385)" \
        "rc=0 out=ZEPHYR_SDK_INSTALL_DIR=$tmp/opt/zephyr-sdk-$want"

    got="$(probe "$tmp/home-other" mps2_an385)"
    expect "only another version registered: an SDK board refuses" "$got" "rc=1 out="
    grep -q 'Remedy: just setup zephyr' "$tmp/err" \
        || fail "the refusal names no remedy: $(cat "$tmp/err")"

    expect "only another version registered: native_sim needs no SDK" \
        "$(probe "$tmp/home-other" native_sim/native/64)" "rc=0 out="

    expect "native_sim under a caller's variant=zephyr still needs no pin" \
        "$(probe "$tmp/home-other" native_sim ZEPHYR_TOOLCHAIN_VARIANT=zephyr)" "rc=0 out="

    expect "a caller's ZEPHYR_SDK_INSTALL_DIR wins, at any version" \
        "$(probe "$tmp/home-other" mps2_an385 ZEPHYR_SDK_INSTALL_DIR="$tmp/opt/zephyr-sdk-$other")" \
        "rc=0 out=ZEPHYR_SDK_INSTALL_DIR=$tmp/opt/zephyr-sdk-$other"

    expect "a third-party variant asks nothing of the SDK" \
        "$(probe "$tmp/home-other" mps2_an385 ZEPHYR_TOOLCHAIN_VARIANT=gnuarmemb)" "rc=0 out="

    # The resolver's store arm names the `[tool.*]` entry's version, not the one
    # asked for. A store holding the 3.7 pin must not answer for another version.
    store="$tmp/store"
    store_sdk="$(NROS_HOME="$store" bash "$repo_root/scripts/lib/zephyr-sdk.sh" store-dir)"
    case "$store_sdk" in
        "$store"/*) ;;
        *) fail "the store dir does not follow NROS_HOME: \`$store_sdk\`" ;;
    esac
    mk_sdk "$store_sdk" "$(basename "$store_sdk" | sed 's/^zephyr-sdk-//')"
    got="$(NROS_HOME="$store" ZEPHYR_SDK_INSTALL_DIR= \
        bash "$repo_root/scripts/lib/zephyr-sdk.sh" --version "$want" --root "$tmp/no-checkout" resolve)" || true
    [ -z "$got" ] || fail "resolve --version $want answered another version's store dir: \`$got\`"
    got="$(NROS_HOME="$store" ZEPHYR_SDK_INSTALL_DIR= \
        bash "$repo_root/scripts/lib/zephyr-sdk.sh" --root "$tmp/no-checkout" resolve)" || true
    [ "$got" = "$store_sdk" ] || fail "resolve of the store's own version: got \`$got\`, want \`$store_sdk\`"
}
selftest

# Every call site tests the status. The accepted shapes are `if ! x="$(…)"` and
# `x="$(…)" || …`; anything else drops the refusal on the floor.
_gg=""  # set by nros_git_grep (printf -v)
nros_git_grep _gg -C "$repo_root" -n 'nros_zephyr_sdk_env "' -- scripts just \
    ':!scripts/build/zephyr-toolchain.sh' ':!scripts/check-zephyr-sdk-env.sh'
while IFS= read -r site; do
    [ -n "$site" ] || continue
    line="${site#*:*:}"
    case "$line" in
        *'if ! '*'nros_zephyr_sdk_env'*) ;;
        *'nros_zephyr_sdk_env'*'||'*) ;;
        *) fail "a caller ignores the helper's status: $site" ;;
    esac
done <<<"$_gg"

if [ "$fails" -ne 0 ]; then
    echo "check-zephyr-sdk-env: $fails failure(s)" >&2
    exit 1
fi
echo "check-zephyr-sdk-env: OK -- the pin is named, refused when missing, and every caller tests it"
