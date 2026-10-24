#!/usr/bin/env bash
# check-zephyr-checkout-guard — the two configure-time refusals in
# `zephyr/cmake/nros_checkout_guard.cmake`, run row by row (phase-478 I2).
#
# Both functions exist for a failure no build in a lane can reach: the mix only
# happens on a host where a SECOND checkout provisioned something shared, and no
# CI job has two checkouts. So the rule is exercised here, under `cmake -P`,
# over synthetic checkouts in a scratch dir OUTSIDE this tree — inside it, every
# synthetic path would also belong to this checkout, which is the nested-worktree
# case issue 1391 measured, not the rows below.
#
#   nros_assert_sdk_not_foreign:   store / this checkout / none / another checkout
#   nros_assert_single_checkout:   copied-out app / this checkout / another
#
# Each row asserts its verdict, so a guard that stopped refusing fails here.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
GUARD="$ROOT/zephyr/cmake/nros_checkout_guard.cmake"
MARKER="packages/core/nros-core/Cargo.toml"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
for c in a b; do
    mkdir -p "$tmp/$c/${MARKER%/*}" "$tmp/$c/zephyr" "$tmp/$c/toolchains/sdk"
    : > "$tmp/$c/$MARKER"
done
mkdir -p "$tmp/store/zephyr-sdk-0.16.8" "$tmp/app"
# The in-checkout SDK dir is `toolchains/sdk`, not the legacy
# `scripts/zephyr/sdk/…` spelling: the guard decides by which checkout OWNS the
# path, never by its name, and that spelling has exactly two owners
# (`check-zephyr-sdk-path-asked`).

fail=0
# row <label> <want: accept|refuse> <cmake call> [guard file]
row() {
    local label="$1" want="$2" call="$3" guard="${4:-$GUARD}" got rc=0
    printf 'include("%s")\n%s\n' "$guard" "$call" > "$tmp/t.cmake"
    cmake -P "$tmp/t.cmake" > "$tmp/out" 2>&1 || rc=$?
    if [ "$rc" -eq 0 ]; then got=accept; else got=refuse; fi
    if [ "$got" = "$want" ]; then
        echo "  ok    $label: $got"
    else
        echo "  FAIL  $label: wanted $want, got $got" >&2
        sed 's/^/        /' "$tmp/out" >&2
        fail=1
    fi
}

mod="$tmp/a/zephyr/.."

# The negative control, on every run: a guard whose refusal was downgraded to a
# STATUS line must make a `refuse` row FAIL here. Without it, a harness that
# could not tell a refusal from an acceptance would print eight `ok`s.
selftest() {
    local broken="$tmp/broken_guard.cmake" saved=$fail
    sed 's/message(FATAL_ERROR/message(STATUS/' "$GUARD" > "$broken"
    row "selftest: downgraded guard" refuse \
        "nros_assert_sdk_not_foreign(\"$mod\" \"$tmp/b/toolchains/sdk\")" \
        "$broken" 2>/dev/null
    if [ "$fail" -eq "$saved" ]; then
        echo "  FAIL  selftest: a guard that no longer refuses passed the refuse row" >&2
        fail=1
    else
        echo "  ok    selftest: a downgraded guard is caught"
        fail=$saved
    fi
}
selftest
row "SDK in the store"            accept "nros_assert_sdk_not_foreign(\"$mod\" \"$tmp/store/zephyr-sdk-0.16.8\")"
row "SDK in this checkout"        accept "nros_assert_sdk_not_foreign(\"$mod\" \"$tmp/a/toolchains/sdk\")"
row "no Zephyr SDK"               accept "nros_assert_sdk_not_foreign(\"$mod\" \"\")"
row "SDK in another checkout"     refuse "nros_assert_sdk_not_foreign(\"$mod\" \"$tmp/b/toolchains/sdk\")"
row "another checkout, allowed"   accept "set(NROS_ALLOW_FOREIGN_SDK ON)
nros_assert_sdk_not_foreign(\"$mod\" \"$tmp/b/toolchains/sdk\")"
row "app copied out"              accept "nros_assert_single_checkout(\"$mod\" \"$tmp/app\")"
row "app in this checkout"        accept "nros_assert_single_checkout(\"$mod\" \"$tmp/a\")"
row "app in another checkout"     refuse "nros_assert_single_checkout(\"$mod\" \"$tmp/b\")"

if [ "$fail" -ne 0 ]; then
    echo "check-zephyr-checkout-guard: FAIL" >&2
    exit 1
fi
echo "check-zephyr-checkout-guard: OK (8 rows over nros_checkout_guard.cmake)"
