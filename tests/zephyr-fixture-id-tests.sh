#!/usr/bin/env bash
# Issue 2835 — `NROS_FIXTURE_ID=<row id>` narrows the Zephyr west leaves to
# that row's one leaf, and an id with no west leaf selects none (exit 3), as
# every other fixture stage does. Before, the emitter read only
# NROS_ZEPHYR_FIXTURE_FILTER, so the refusal hint's own spelling built every
# leaf. Records mode only: nothing is configured or built.
#
# Issue 2987 -- the narrowing is the emitter's `--fixture-id` FLAG, which the
# Zephyr lane passes from NROS_FIXTURE_ID. The emitter is also the
# whole-manifest inventory that gates read, so an INHERITED NROS_FIXTURE_ID (or
# NROS_ZEPHYR_FIXTURE_FILTER) must change nothing: the negative controls below
# require byte-identical output with each variable set and unset.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
emit="$root/scripts/build/zephyr-fixture-leaves.sh"
# shellcheck source=scripts/lib/grep-q.sh
source "$root/scripts/lib/grep-q.sh"
pass=0 fail=0
ok() { pass=$((pass + 1)); echo "  ok   $1"; }
bad() { fail=$((fail + 1)); echo "  FAIL $1"; }
records() { grep -c $'^fixture\t' || true; }

# `env -u`: the baseline is the emitter with NO narrowing in its environment,
# whatever this test inherited.
base_out="$(env -u NROS_FIXTURE_ID -u NROS_ZEPHYR_FIXTURE_FILTER bash "$emit" --emit records --include-workspace-entry 2>/dev/null)"
all="$(records <<<"$base_out")"
[ "$all" -gt 1 ] && ok "unset: every leaf ($all)" || bad "unset: expected many leaves, got $all"

# Two Zephyr workspace rows of different shapes (a C sched-task entry and a
# Rust pair image). Only `[[workspace_fixture]]` rows carry an `id`: the nine
# `[[fixture]]` west rows have none, so `NROS_FIXTURE_ID` cannot name them.
for row in workspace-zephyr-c-realtime-ctf workspace-rust-zephyr-talker-clock-sntp; do
    leaf="$(python3 "$root/scripts/build/fixtures-manifest.py" west-leaf-id --id "$row")"
    rc=0
    out="$(bash "$emit" --emit records --include-workspace-entry --fixture-id "$row" 2>&1)" || rc=$?
    n="$(records <<<"$out")"
    if [ "$rc" -eq 0 ] && [ "$n" -eq 1 ] && nros_grep_q $'^fixture\t'"$leaf"$'\t' <<<"$out"; then
        ok "$row: exactly its one leaf ($leaf)"
    else
        bad "$row: rc=$rc, $n record(s), want 1 naming $leaf"
    fi
done

rc=0
out="$(bash "$emit" --emit records --include-workspace-entry --fixture-id no-such-row 2>&1)" || rc=$?
if [ "$rc" -eq 3 ] && nros_grep_q "NROS_FIXTURE_ID=no-such-row" <<<"$out" && [ "$(records <<<"$out")" -eq 0 ]; then
    ok "an id with no west leaf selects none, exit 3, naming the id"
else
    bad "bogus id: rc=$rc, $(records <<<"$out") record(s); want exit 3, none, and a message naming it"
fi

# Issue 2987's negative controls: an inherited narrowing is not a narrowing.
# Each variable alone, then both, must leave the whole-manifest output exactly
# as it is with neither set -- the verdict every inventory gate reads.
for inherited in "NROS_FIXTURE_ID=workspace-zephyr-c-realtime-ctf" \
    "NROS_FIXTURE_ID=no-such-row" \
    "NROS_ZEPHYR_FIXTURE_FILTER=no-leaf-matches-this" \
    "NROS_FIXTURE_ID=workspace-zephyr-c-realtime-ctf NROS_ZEPHYR_FIXTURE_FILTER=xrce"; do
    rc=0
    # shellcheck disable=SC2086 -- the assignments are split into env's arguments on purpose.
    out="$(env -u NROS_FIXTURE_ID -u NROS_ZEPHYR_FIXTURE_FILTER $inherited \
        bash "$emit" --emit records --include-workspace-entry 2>/dev/null)" || rc=$?
    if [ "$rc" -eq 0 ] && [ "$out" = "$base_out" ]; then
        ok "inherited $inherited: the whole manifest, unchanged"
    else
        bad "inherited $inherited: rc=$rc, $(records <<<"$out") record(s) against $all unset -- the environment narrowed the inventory"
    fi
done

if python3 "$root/scripts/build/fixtures-manifest.py" west-leaf-id --id no-such-row >/dev/null; then
    bad "west-leaf-id answered an id no west row has"
else
    ok "west-leaf-id refuses an id no west row has"
fi

echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
