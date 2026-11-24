#!/usr/bin/env bash
#
# issue 2661 — `runner-sweep` must stop evicting on the MEASURED remainder of
# an area, not on a running sum of per-unit sizes.
#
# `du` counts a hard-linked inode once per invocation, so a file shared by N
# units is in each unit's own `du` and only once in the area's. West
# workspaces and zephyr build trees are hard-linked (`cp -al`), so the summed
# "freed" passed the area total after the first few units. The sweep then
# stopped with the area still over budget, and its over-budget warning (which
# read the same sum) stayed silent.
#
# This runs a REAL sweep (it deletes), in a sandbox it builds, over units
# that share one hard-linked file, and asserts what is left ON DISK.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sweep="$repo_root/scripts/ci/runner-sweep.sh"
failures=0
checks=0

ok()   { echo "  [ok]   $1"; checks=$((checks + 1)); }
fail() { echo "  [FAIL] $1" >&2; failures=$((failures + 1)); checks=$((checks + 1)); }

[ -x "$sweep" ] || { echo "FATAL: $sweep missing or not executable" >&2; exit 2; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# A gate may only assert on what it built: the sweep reads every root from the
# environment first, and this run DELETES. Clear the inherited family, then
# point every HOME-derived default at the sandbox too.
for _var in $(compgen -e); do
    case "$_var" in NROS_*|RUNNER_*|ZEPHYR_*|SCCACHE_*|XDG_*) unset "$_var" ;; esac
done
unset _var
mkdir -p "$work/home" "$work/no-work" "$work/sccache" "$work/state"
export HOME="$work/home"
export NROS_HOME="$work/home/.nros"
export NROS_SWEEP_WORK="$work/no-work"
export SCCACHE_DIR="$work/sccache"
export NROS_SWEEP_STATE_DIR="$work/state"
export NROS_RUNNER_REPO_ROOT="$work"
export NROS_BUILD_ROOT="$work/build"

# Three fixture units that hard-link ONE 4 MiB file, and a dot-named child
# holding 1 MiB. The dot child is never an eviction unit, so after evicting
# every unit the area really holds 1 MiB, which is over a 0 budget.
fx="$work/build/cargo-fixtures"
mkdir -p "$fx/u1/deps" "$fx/u2/deps" "$fx/u3/deps" "$fx/.lock"
dd if=/dev/zero of="$fx/u1/deps/shared.rlib" bs=1M count=4 status=none
ln "$fx/u1/deps/shared.rlib" "$fx/u2/deps/shared.rlib"
ln "$fx/u1/deps/shared.rlib" "$fx/u3/deps/shared.rlib"
dd if=/dev/zero of="$fx/.lock/held" bs=1M count=1 status=none
# One inode, so one mtime: the three units tie on age, and which goes first
# does not matter here. What matters is whether the loop stops early.

# The premise, asserted: the three units' own `du` sum to about three times
# what one `du` over all of them reports. Without that this models nothing.
sum=0
for u in u1 u2 u3; do
    sum=$((sum + $(du -sx --block-size=1 "$fx/$u" | awk '{print $1}')))
done
union="$(du -scx --block-size=1 "$fx/u1" "$fx/u2" "$fx/u3" | awk 'END {print $1}')"
if [ "$sum" -gt $((union * 2)) ]; then
    ok "premise: per-unit du sums to $sum B, one du over the units to $union B"
else
    fail "premise: the hard link is not shared (sum $sum B, union $union B)"
fi

out="$(PATH="/usr/bin:/bin" NROS_SWEEP_BUDGET_FIXTURES_GIB=0 \
       "$sweep" --disk 2>&1)" || true

left=""
for u in u1 u2 u3; do
    [ -e "$fx/$u" ] && left="$left $u"
done
if [ -z "$left" ]; then
    ok "every unit sharing the hard link is evicted (the loop stops on what is on disk)"
else
    fail "units left on disk with the area over budget:$left
$out"
fi

if [ -e "$fx/.lock/held" ]; then
    ok "the dot-named child, never an eviction unit, is left alone"
else
    fail "the dot-named child was deleted:
$out"
fi

if [[ "$out" == *'fixtures is still over budget after eviction'* ]]; then
    ok "the over-budget warning fires on the measured remainder"
else
    fail "no over-budget warning, with 1 MiB measured against a 0 B budget:
$out"
fi

# The cost half of the same loop: the remainder is re-MEASURED only when the
# running estimate says it may be under budget, not after every eviction (one
# `du` over the whole area per eviction was the step's 55 minutes on 118).
# Twenty independent units, all evicted against a 0 budget: the estimate never
# reaches 0 (the dot child stays), so the area is measured twice, once before
# and once after, where the per-eviction rule measured it 21 times. A `du` shim
# on PATH counts the whole-area (`-c`) measurements.
fx2="$work/build/cargo-fixtures"
rm -rf "$fx2"
mkdir -p "$fx2/.lock" "$work/shim"
for i in $(seq 1 20); do
    mkdir -p "$fx2/v$i"
    dd if=/dev/zero of="$fx2/v$i/blob" bs=64K count=1 status=none
done
dd if=/dev/zero of="$fx2/.lock/held" bs=64K count=1 status=none
real_du="$(command -v du)"
printf '#!/bin/sh
case " $* " in *" -scx "*) echo x >> "%s";; esac
exec %s "$@"
' \
    "$work/du-calls" "$real_du" > "$work/shim/du"
chmod +x "$work/shim/du"
: > "$work/du-calls"
out="$(PATH="$work/shim:/usr/bin:/bin" NROS_SWEEP_BUDGET_FIXTURES_GIB=0 \
       "$sweep" --disk 2>&1)" || true
left2=0
for i in $(seq 1 20); do [ -e "$fx2/v$i" ] && left2=$((left2 + 1)); done
area_du="$(grep -c . "$work/du-calls")"
if [ "$left2" -eq 0 ]; then
    ok "all twenty over-budget units are evicted, as before"
else
    fail "$left2 unit(s) left with the area over budget:
$out"
fi
if [ "$area_du" -le 4 ]; then
    ok "the area is re-measured $area_du time(s) for 20 evictions, not once per eviction"
else
    fail "the area was re-measured $area_du times for 20 evictions: the loop re-reads after every one"
fi

echo ""
if [ "$failures" -eq 0 ]; then
    echo "runner-sweep-hardlink: $checks check(s) passed"
    exit 0
fi
echo "runner-sweep-hardlink: $failures of $checks check(s) FAILED" >&2
echo "" >&2
echo "  A sweep that sums per-unit du counts a hard-linked file once per unit," >&2
echo "  stops while the area is still over budget, and says nothing (issue 2661)." >&2
exit 1
