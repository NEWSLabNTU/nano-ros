#!/usr/bin/env bash
# Issue 1325 -- `NROS_ZEPHYR_FIXTURE_FILTER` sees each row's FIELDS, never the
# path of the checkout it runs in, and never a line that `.*` can cross.
#
# Both Zephyr builders used to match the filter against one joined string that
# held the leaves' ABSOLUTE conf paths, so in a worktree named `448-xrce` the
# filter `xrce` selected every leaf (57, zenoh and cyclonedds included), and
# `build-(c|cpp)-.*-xrce` paired fields across the line. Both failures select
# too MANY leaves, which the lane reports as normal work, at ~10 minutes a leaf.
#
# The negative control is the issue's own: run the REAL leaf emitter from a
# checkout path whose name contains `xrce` (a symlink to this checkout, which
# `cd && pwd` keeps), and require that `xrce` selects exactly the leaves the
# anchored spelling `/xrce$` does — and that the cross-field pattern selects
# only C/C++ xrce leaves. Then the shared helper is held to the cases the
# second builder (`west-fixtures.sh`) depends on, since its rows need a west
# workspace to enumerate.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/fixture-filter.sh
. "$repo_root/scripts/lib/fixture-filter.sh"

fail=0
bad() {
    echo "check-fixture-filter-fields: FAIL -- $*" >&2
    fail=1
}

tmp="$(mktemp -d)"
trap 'rm -f "$tmp/448-xrce"; rmdir "$tmp"' EXIT
ln -s "$repo_root" "$tmp/448-xrce"

leaves() {
    bash "$repo_root/scripts/build/zephyr-fixture-leaves.sh" --emit records \
        --nros-root "$tmp/448-xrce" --filter "$1" | cut -f2
}

bare="$(leaves xrce | sort)"
anchored="$(leaves '/xrce$' | sort)"
[ -n "$anchored" ] || bad "'/xrce\$' selected no leaf; the control measures nothing"
if [ "$bare" != "$anchored" ]; then
    bad "from a checkout named 448-xrce, 'xrce' selected $(printf '%s\n' "$bare" | wc -l) leaves where '/xrce\$' selects $(printf '%s\n' "$anchored" | wc -l): the checkout path reached the filter"
fi
while IFS= read -r id; do
    [ -n "$id" ] || continue
    case "$id" in
        */c/*/xrce | */cpp/*/xrce) ;;
        *) bad "'build-(c|cpp)-.*-xrce' selected $id: the pattern crossed fields" ;;
    esac
done < <(leaves 'build-(c|cpp)-.*-xrce')

# The helper's selftest, against the shapes `west-fixtures.sh` passes. Run on
# every invocation, not behind a flag.
root=/x/448-xrce
expect() {
    local want="$1" filter="$2"
    shift 2
    local got=no
    if nros_fixture_filter_selects "$filter" "$root" "$@"; then got=yes; fi
    [ "$got" = "$want" ] || bad "helper: '$filter' over [$*] gave $got, want $want"
}
selftest() {
    expect yes zephyr_self_pkg_sibling native_sim/native/64 zephyr_self_pkg_sibling \
        packages/testing/nros-tests/fixtures/zephyr_self_pkg/sibling/caller "-DCONF_FILE=prj.conf"
    expect no xrce native_sim/native/64 build-rust-talker-zenoh examples/zephyr/rust/talker \
        "prj.conf;prj-zenoh.conf;$root/cmake/zephyr/qemu-cortex-m3.conf" zephyr/rust/talker/zenoh
    expect yes 'qemu-cortex-m3' native_sim/native/64 build-rust-talker-zenoh \
        "prj.conf;$root/cmake/zephyr/qemu-cortex-m3.conf"
    expect no 'build-c-.*-xrce' build-c-talker-zenoh zephyr/c/talker/xrce
    expect yes '^prj-xrce\.conf$' "prj.conf;prj-xrce.conf"
}
selftest

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "check-fixture-filter-fields: OK -- the Zephyr fixture filter matches per field, repo-relative"
