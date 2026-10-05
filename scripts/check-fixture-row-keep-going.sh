#!/usr/bin/env bash
# Issue 1671 — `nros_fixture_row` keeps a lane building past a failed row ONLY
# when the lane asks, never swallows a failure, and is the road every row of
# the two shared builders takes.
#
# Negative controls on the normal path, because each property below is one a
# plausible edit breaks silently:
#
#   A. unset ledger: a failed row's status reaches the caller and `set -e`
#      ends the builder there (every developer invocation, unchanged).
#   B. ledger set: the failed row is recorded, the NEXT row still runs, and
#      the call returns 0.
#   C. ledger set: `set -e` still holds INSIDE a row. The obvious spelling
#      (`row || rc=$?`) suspends -e for the whole row body, so a row whose
#      middle step fails would run on and "pass" on its last command.
#   D. `fixture-rows-keep-going.sh` fails when the ledger is not empty, passes
#      when it is, and keeps the command's own failure status.
#   E. reach: both builders call their row functions through the wrapper and
#      nowhere directly — a second, unwrapped call site is a row that still
#      ends the lane.
set -euo pipefail
cd "$(dirname "$0")/.."

GATE=check-fixture-row-keep-going
fail=0
bad() { echo "$GATE: FAIL — $*" >&2; fail=1; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-row-ledger.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

lib="$PWD/scripts/build/fixture-row-ledger.sh"

# A — unset: the failure propagates and -e stops the script.
out="$(bash -c "set -e; source '$lib'; r(){ return 3; }; nros_fixture_row a r; echo AFTER" 2>&1)" && rc=0 || rc=$?
[ "$rc" = 3 ] || bad "A: unset ledger returned rc=$rc, want 3 (the row's own status)"
case "$out" in *AFTER*) bad "A: unset ledger kept going past a failed row" ;; esac

# B — set: recorded, next row runs, rc 0.
ledger="$tmp/b.tsv"
out="$(NROS_FIXTURE_FAILED_ROWS="$ledger" bash -c "set -e; source '$lib'
r(){ return 4; }; ok(){ echo NEXT_RAN; }
nros_fixture_row 'row one' r; nros_fixture_row 'row two' ok; echo DONE" 2>&1)" && rc=0 || rc=$?
[ "$rc" = 0 ] || bad "B: keep-going returned rc=$rc, want 0"
case "$out" in *NEXT_RAN*DONE*) ;; *) bad "B: the next row did not run: $out" ;; esac
grep -qx $'row one\trc=4' "$ledger" 2>/dev/null || bad "B: ledger lacks the failed row: $(cat "$ledger" 2>/dev/null)"
[ "$(grep -c . "$ledger")" = 1 ] || bad "B: ledger records more than the one failed row"

# C — set -e inside a kept-going row.
ledger="$tmp/c.tsv"
out="$(NROS_FIXTURE_FAILED_ROWS="$ledger" bash -c "set -e; source '$lib'
r(){ false; echo SWALLOWED; }
nros_fixture_row 'row c' r" 2>&1)" || true
case "$out" in *SWALLOWED*) bad "C: set -e was suspended inside the row — its failure ran on" ;; esac
grep -q '^row c' "$ledger" 2>/dev/null || bad "C: a row failing mid-body was not recorded"

# D — the owner.
owner="scripts/ci/fixture-rows-keep-going.sh"
bash "$owner" "$tmp/d1.tsv" -- true >/dev/null 2>&1 || bad "D: an all-green step failed"
bash "$owner" "$tmp/d2.tsv" -- bash -c "printf 'x\trc=1\n' >> \"\$NROS_FIXTURE_FAILED_ROWS\"" \
    >/dev/null 2>&1 && bad "D: a step with a failed row passed"
rc=0; bash "$owner" "$tmp/d3.tsv" -- bash -c 'exit 7' >/dev/null 2>&1 || rc=$?
[ "$rc" = 7 ] || bad "D: the command's own failure became rc=$rc, want 7"
printf 'stale\trc=1\n' > "$tmp/d4.tsv"
bash "$owner" "$tmp/d4.tsv" -- true >/dev/null 2>&1 || bad "D: a stale ledger from a previous run failed a green step"

# E — reach.
fb=scripts/build/fixtures-build.sh
ws=scripts/build/workspace-fixtures-build.sh
grep -nE '^[[:space:]]*"\$fn" "\$line"' "$fb" && bad "E: $fb calls a row directly, not through nros_fixture_row"
grep -q 'nros_fixture_row "$(nros_fixture_row_label "$line")" "$fn" "$line"' "$fb" \
    || bad "E: $fb's serial loop does not go through nros_fixture_row"
grep -q '+@nros_fixture_row ' "$fb" || bad "E: $fb's make rows do not go through nros_fixture_row"
direct="$(grep -nE '^[[:space:]]*build_workspace "\$record"' "$ws" || true)"
[ -z "$direct" ] || bad "E: $ws calls build_workspace directly: $direct"
[ "$(grep -c 'nros_ws_row "$record"' "$ws")" -ge 2 ] \
    || bad "E: $ws's row loops do not go through nros_ws_row"

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "$GATE: OK — kept-going rows are recorded, -e holds inside them, the owner fails on any, and both builders route every row through the wrapper."
