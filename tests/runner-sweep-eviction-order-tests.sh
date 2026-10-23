#!/usr/bin/env bash
#
# issue 1094 — `runner-sweep` must evict the entry that was least recently
# WRITTEN, not the one whose directory inode is oldest.
#
# The two are different for exactly the entries that hold the most bytes. A
# directory's mtime moves only when an entry is created or removed IN THAT
# DIRECTORY, and a cargo `--target-dir` writes into `<profile>/deps/…` several
# levels down — so a shared fixture group's dir mtime freezes near creation and
# stays there while the group is used every day.
#
# Measured on 2026-09-05 before the fix: eleven of the twenty children of
# `build/cargo-fixtures` were wrong by ~20 days, and the sweep proposed deleting
# `cargo-fixtures/linux` (12.8 GiB, the NATIVE platform group every tier-1 run
# needs, written to the previous day) to reclaim a 60 MiB overrun.
#
# This builds that exact shape and asserts the order, because the failure is
# silent (issue 1353's area coverage is checked at the end, same reason): the sweep reports a successful eviction either way, and what it cost
# only shows up as the next job rebuilding a fixture nobody touched.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sweep="$repo_root/scripts/ci/runner-sweep.sh"
# shellcheck source=scripts/lib/grep-q.sh
. "$repo_root/scripts/lib/grep-q.sh"
failures=0
checks=0

ok()   { echo "  [ok]   $1"; checks=$((checks + 1)); }
fail() { echo "  [FAIL] $1" >&2; failures=$((failures + 1)); checks=$((checks + 1)); }

[ -x "$sweep" ] || { echo "FATAL: $sweep missing or not executable" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# Two eviction units, both with an OLD directory mtime, differing only in when
# their contents were last written. `cargo-fixtures` is one of the kinds the
# fixtures area collects, so this exercises the real code path.
fx="$work/build/cargo-fixtures"
mkdir -p "$fx/busy/nros-relwithdebinfo/deps" "$fx/cold/nros-relwithdebinfo/deps"

# ~5 MiB each, so both are worth evicting and the choice is about age alone.
dd if=/dev/zero of="$fx/busy/nros-relwithdebinfo/deps/libbusy.rlib" bs=1M count=5 status=none
dd if=/dev/zero of="$fx/cold/nros-relwithdebinfo/deps/libcold.rlib" bs=1M count=5 status=none

# `busy` was WRITTEN today; `cold` was written 40 days ago. Both directory
# inodes are stamped 40 days old — which is the whole point: before the fix the
# two were indistinguishable, and `busy` sorted first often enough to be deleted.
old="$(date -d '40 days ago' +%Y%m%d%H%M)"
touch -t "$old" "$fx/cold/nros-relwithdebinfo/deps/libcold.rlib"
touch -t "$old" "$fx/busy" "$fx/cold" \
      "$fx/busy/nros-relwithdebinfo" "$fx/cold/nros-relwithdebinfo" \
      "$fx/busy/nros-relwithdebinfo/deps" "$fx/cold/nros-relwithdebinfo/deps"

# The premise, asserted rather than assumed: if the directory mtimes ever stop
# being equal, this fixture no longer models the hazard and a pass means nothing.
if [ "$(stat -c '%Y' "$fx/busy")" = "$(stat -c '%Y' "$fx/cold")" ]; then
    ok "premise: both entries carry the same directory mtime"
else
    fail "premise: the directory mtimes differ, so this no longer models issue 1094"
fi

# A 1 MiB budget against ~10 MiB used: over budget, and one eviction is not
# enough, so the ORDER is what the assertions below read.
out="$(NROS_RUNNER_REPO_ROOT="$work" \
       NROS_SWEEP_BUDGET_FIXTURES_GIB=0 \
       NROS_SWEEP_STATE_DIR="$work/state" \
       "$sweep" --check --disk 2>&1)" || true

busy_line="$(printf '%s\n' "$out" | grep -n 'rm -rf .*/busy' | head -1 | cut -d: -f1)"
cold_line="$(printf '%s\n' "$out" | grep -n 'rm -rf .*/cold' | head -1 | cut -d: -f1)"

if [ -n "$cold_line" ]; then
    ok "the entry whose CONTENT is old is proposed for eviction"
else
    fail "the cold entry was never proposed — the fixtures area did not run:
$out"
fi

if [ -n "$cold_line" ] && [ -n "$busy_line" ]; then
    if [ "$cold_line" -lt "$busy_line" ]; then
        ok "cold is evicted BEFORE busy — ordering follows content, not the inode"
    else
        fail "busy (written today) is evicted before cold (written 40d ago):
$out"
    fi
elif [ -n "$cold_line" ] && [ -z "$busy_line" ]; then
    ok "only the cold entry is proposed; the busy one is left alone"
fi

# And the negative control: with the old rule the two are indistinguishable, so
# assert the helper itself separates them. This is what actually regressed.
newest_busy="$(bash -c 'source "$0"; _newest_mtime "$1"' "$sweep" "$fx/busy" 2>/dev/null | tail -1 || true)"
if [ -z "${newest_busy//[0-9]/}" ] && [ -n "$newest_busy" ]; then
    if [ "$newest_busy" -gt "$(stat -c '%Y' "$fx/busy")" ]; then
        ok "_newest_mtime reports content time, which is newer than the inode"
    else
        fail "_newest_mtime returned the directory's own mtime ($newest_busy)"
    fi
else
    echo "  [skip] _newest_mtime not separately callable (sourcing runs the sweep)"
fi

# --- issue 1353: the caches one level deeper, and what no area holds ----------
#
# `sizes-probe/<rustc-slug>/<key>` and `zephyr-workspace-builds/<line>/build-*`
# were in no area, so the sweep neither evicted nor reported them while they
# were the two largest consumers on a runner that ran out of disk. And a leaf
# build under `examples/` is in no area by design, so it must be REPORTED: the
# sweep printed "done" over 250 GiB it never looked at.
w2="$work/areas"
mkdir -p "$w2/build/sizes-probe/rustc-old/k1" "$w2/build/sizes-probe/rustc-new/k2" \
         "$w2/build/zephyr-workspace-builds/3.7/build-leaf" "$w2/examples/ws/build"
dd if=/dev/zero of="$w2/build/sizes-probe/rustc-old/k1/lib.rlib" bs=1M count=3 status=none
dd if=/dev/zero of="$w2/build/sizes-probe/rustc-new/k2/lib.rlib" bs=1M count=3 status=none
dd if=/dev/zero of="$w2/build/zephyr-workspace-builds/3.7/build-leaf/zephyr.elf" bs=1M count=3 status=none
dd if=/dev/zero of="$w2/examples/ws/build/app.o" bs=1M count=2 status=none
touch -t "$old" "$w2/build/sizes-probe/rustc-old/k1/lib.rlib"

out2="$(NROS_RUNNER_REPO_ROOT="$w2" NROS_BUILD_ROOT="$w2/build" \
        NROS_SWEEP_BUDGET_FIXTURES_GIB=0 NROS_SWEEP_UNBUDGETED_SHOW_MIB=1 \
        NROS_SWEEP_UNBUDGETED_WARN_GIB=0 NROS_SWEEP_STATE_DIR="$w2/state" \
        "$sweep" --check --disk 2>&1)" || true

old_line="$(printf '%s\n' "$out2" | grep -n 'rm -rf .*/rustc-old/k1' | head -1 | cut -d: -f1)"
new_line="$(printf '%s\n' "$out2" | grep -n 'rm -rf .*/rustc-new/k2' | head -1 | cut -d: -f1)"
if [ -n "$old_line" ] && [ -n "$new_line" ] && [ "$old_line" -lt "$new_line" ]; then
    ok "a sizes-probe key is an eviction unit, and the retired toolchain's goes first"
else
    fail "sizes-probe keys were not evicted oldest-first:
$out2"
fi
if nros_grep_q 'rm -rf .*/zephyr-workspace-builds/3.7/build-leaf' <<<"$out2"; then
    ok "a west build leaf under zephyr-workspace-builds/<line> is an eviction unit"
else
    fail "the west build leaf was never proposed:
$out2"
fi
if nros_grep_q -E '^ +unbudgeted: 2 MiB in no area' <<<"$out2" \
   && nros_grep_q -E '^ +2 MiB +.*/examples$' <<<"$out2"; then
    ok "the leaf build in no area is reported and named, and no area is counted twice"
else
    fail "unbudgeted report missing, mis-sized or unnamed (want 2 MiB, examples):
$out2"
fi
if nros_grep_q 'is outside every area' <<<"$out2"; then
    ok "unbudgeted space past the warn threshold warns"
else
    fail "no warning at a 0 GiB warn threshold:
$out2"
fi
if nros_grep_q 'Broken pipe' <<<"$out2"; then
    fail "sort wrote into a closed pipe:
$out2"
else
    ok "no Broken pipe noise from the eviction listing"
fi

echo ""
if [ "$failures" -eq 0 ]; then
    echo "runner-sweep-eviction-order: $checks check(s) passed"
    exit 0
fi
echo "runner-sweep-eviction-order: $failures of $checks check(s) FAILED" >&2
echo "" >&2
echo "  Evicting by directory mtime deletes the BUSIEST fixture group, because" >&2
echo "  the group that has existed longest has both the oldest inode and the" >&2
echo "  most accumulated bytes (issue 1094)." >&2
exit 1
