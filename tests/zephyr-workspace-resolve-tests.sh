#!/usr/bin/env bash
# Issue 2308 — `scripts/lib/zephyr-workspace.sh resolve` answers "is a west
# workspace provisioned", and a Zephyr SOURCE tree is not one.
#
# CI's fast job sparse-clones Zephyr's C++ minimal-libc headers into
# `zephyr-workspace/zephyr`. That tree has a `zephyr/` and no `.west/`, and it
# used to resolve, so `template-copy-out` ran a west build against it and
# failed where it should have reported "Zephyr not provisioned". Each row
# builds a candidate tree under a temp root, with the store pointed at an
# empty temp dir so this host's real workspaces cannot answer.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
resolver="$here/scripts/lib/zephyr-workspace.sh"
work="$(mktemp -d "${TMPDIR:-/tmp}/zephyr-ws-resolve-XXXXXX")"
trap 'rm -rf "$work"' EXIT
root="$work/nano-ros"
mkdir -p "$root" "$work/home"

resolve() {
    env -u NROS_ZEPHYR_WORKSPACE -u ZEPHYR_BASE NROS_HOME="$work/home" \
        bash "$resolver" --root "$root" --version 3.7 resolve "$@"
}

fails=0
check() {
    local what="$1" want="$2" got rc=0
    got="$(resolve 2>/dev/null)" || rc=$?
    if [ "$want" = none ]; then
        if [ "$rc" -eq 0 ]; then
            echo "[FAIL] $what: resolved \`$got\`, expected nothing"
            fails=$((fails + 1))
        else
            echo "  ok    $what: nothing resolves"
        fi
    elif [ "$rc" -ne 0 ] || [ "$got" != "$want" ]; then
        echo "[FAIL] $what: got \`$got\` (rc=$rc), expected \`$want\`"
        fails=$((fails + 1))
    else
        echo "  ok    $what: $got"
    fi
}

check "no tree at all" none

# The sparse header clone CI makes: zephyr/ with one include dir, no .west/.
mkdir -p "$root/zephyr-workspace/zephyr/lib/cpp/minimal/include"
check "a sparse Zephyr source tree (CI's header clone)" none

# The same directory once `west init` has made it a workspace.
mkdir -p "$root/zephyr-workspace/.west"
check "a west workspace" zephyr-workspace

# The override is returned as stated, existing or not (it is the install
# target of `just zephyr setup`).
got="$(NROS_ZEPHYR_WORKSPACE="$work/to-be-installed" NROS_HOME="$work/home" \
    bash "$resolver" --root "$root" --version 3.7 resolve)"
if [ "$got" = "$work/to-be-installed" ]; then
    echo "  ok    the override is returned as stated"
else
    echo "[FAIL] the override: got \`$got\`"
    fails=$((fails + 1))
fi

if [ "$fails" -ne 0 ]; then
    echo "zephyr-workspace-resolve: $fails row(s) failed"
    exit 1
fi
echo "zephyr-workspace-resolve: OK"
