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
#   F. `fixture-row-failed.sh` (a make recipe's way in): unset, it exits the
#      leaf's status; set, it records and exits 0 — run inside a real make,
#      where a failed leaf must not stop the next one.
#   G. the run side: `nros_fixture_build_ledger_lets_cells_run` lets the cells
#      run ONLY when the build's ledger names rows, so a missing fixture with
#      no recorded build failure still stops the run as a precondition.
#   H. reach of the lanes: the Zephyr make driver, the census prepass and the
#      stage reporter each take the ledger road, and every platform-matrix build
#      runs under the owner with its cells step reading the same ledger.
set -euo pipefail
cd "$(dirname "$0")/.."
# Hermetic: every case states its own ledger. `just build <lane>` runs this
# gate INSIDE `fixture-rows-keep-going.sh`, which exports the ledger for the
# whole build, so an inherited NROS_FIXTURE_FAILED_ROWS turned the "unset"
# cases into ledger cases and the gate red on platform-matrix 45339477050.
unset NROS_FIXTURE_FAILED_ROWS NROS_FIXTURE_BUILD_LEDGER

GATE=check-fixture-row-keep-going
fail=0
bad() { echo "$GATE: FAIL — $*" >&2; fail=1; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-row-ledger.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
# Issue 1905 -- case D runs the owner, which appends its failed rows to
# `$GITHUB_STEP_SUMMARY`. Inherited from a CI step, that put this case's fake
# `x rc=1` row on the real job's summary page. Give the owner a file of ours.
export GITHUB_STEP_SUMMARY="$tmp/step-summary"

lib="$PWD/scripts/build/fixture-row-ledger.sh"
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# A-D are the negative controls, run on every invocation (the normal path).
self_test() {
local out rc ledger owner
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
nros_grep_q -x $'row one\trc=4' "$ledger" || bad "B: ledger lacks the failed row: $(cat "$ledger" 2>/dev/null)"
[ "$(grep -c . "$ledger")" = 1 ] || bad "B: ledger records more than the one failed row"

# C — set -e inside a kept-going row.
ledger="$tmp/c.tsv"
out="$(NROS_FIXTURE_FAILED_ROWS="$ledger" bash -c "set -e; source '$lib'
r(){ false; echo SWALLOWED; }
nros_fixture_row 'row c' r" 2>&1)" || true
case "$out" in *SWALLOWED*) bad "C: set -e was suspended inside the row — its failure ran on" ;; esac
nros_grep_q '^row c' "$ledger" || bad "C: a row failing mid-body was not recorded"

# D — the owner.
owner="scripts/ci/fixture-rows-keep-going.sh"
bash "$owner" "$tmp/d1.tsv" -- true >/dev/null 2>&1 || bad "D: an all-green step failed"
bash "$owner" "$tmp/d2.tsv" -- bash -c "printf 'x\trc=1\n' >> \"\$NROS_FIXTURE_FAILED_ROWS\"" \
    >/dev/null 2>&1 && bad "D: a step with a failed row passed"
rc=0; bash "$owner" "$tmp/d3.tsv" -- bash -c 'exit 7' >/dev/null 2>&1 || rc=$?
[ "$rc" = 7 ] || bad "D: the command's own failure became rc=$rc, want 7"
printf 'stale\trc=1\n' > "$tmp/d4.tsv"
bash "$owner" "$tmp/d4.tsv" -- true >/dev/null 2>&1 || bad "D: a stale ledger from a previous run failed a green step"

# F — a make recipe's failed leaf, both ways, in a real make.
local helper="$PWD/scripts/build/fixture-row-failed.sh"
printf '%s\n' 'SHELL := /bin/bash' '.SHELLFLAGS := -eu -o pipefail -c' 'all: a b' \
    'a:' "	@status=0; ( false ) || status=\$\$?; if [ \"\$\$status\" -ne 0 ]; then bash $helper 'leaf a' \"\$\$status\"; exit \$\$?; fi" \
    'b:' '	@echo LEAF_B_BUILT' > "$tmp/Makefile"
rc=0; out="$(env -u NROS_FIXTURE_FAILED_ROWS make -s -f "$tmp/Makefile" all 2>&1)" || rc=$?
[ "$rc" -ne 0 ] || bad "F: with no ledger a failed leaf did not fail make"
ledger="$tmp/f.tsv"
rc=0; out="$(NROS_FIXTURE_FAILED_ROWS="$ledger" make -s -f "$tmp/Makefile" all 2>&1)" || rc=$?
[ "$rc" = 0 ] || bad "F: with a ledger make still failed (rc=$rc): $out"
case "$out" in *LEAF_B_BUILT*) ;; *) bad "F: the leaf after a failed one did not build: $out" ;; esac
nros_grep_q -x $'leaf a\trc=1' "$ledger" || bad "F: the failed leaf was not recorded: $(cat "$ledger" 2>/dev/null)"

# G — the run side.
rc=0; env -u NROS_FIXTURE_BUILD_LEDGER bash -c "source '$lib'; nros_fixture_build_ledger_lets_cells_run g" \
    2>/dev/null || rc=$?
[ "$rc" = 1 ] || bad "G: with no build ledger a missing fixture would not stop the run"
: > "$tmp/g-empty.tsv"
rc=0; NROS_FIXTURE_BUILD_LEDGER="$tmp/g-empty.tsv" bash -c "source '$lib'; nros_fixture_build_ledger_lets_cells_run g" \
    2>/dev/null || rc=$?
[ "$rc" = 1 ] || bad "G: an EMPTY build ledger let the cells run past a missing fixture"
printf 'row g\trc=2\n' > "$tmp/g.tsv"
rc=0; NROS_FIXTURE_BUILD_LEDGER="$tmp/g.tsv" bash -c "source '$lib'; nros_fixture_build_ledger_lets_cells_run g" \
    2>/dev/null || rc=$?
[ "$rc" = 0 ] || bad "G: a build ledger naming rows did not let the cells run"

}
self_test

# E — reach.
fb=scripts/build/fixtures-build.sh
ws=scripts/build/workspace-fixtures-build.sh
grep -nE '^[[:space:]]*"\$fn" "\$line"' "$fb" && bad "E: $fb calls a row directly, not through nros_fixture_row"
nros_grep_q -F 'nros_fixture_row "$(nros_fixture_row_label "$line")" "$fn" "$line"' "$fb" \
    || bad "E: $fb's serial loop does not go through nros_fixture_row"
nros_grep_q -F '+@nros_fixture_row ' "$fb" || bad "E: $fb's make rows do not go through nros_fixture_row"
direct="$(grep -nE '^[[:space:]]*build_workspace "\$record"' "$ws" || true)"
[ -z "$direct" ] || bad "E: $ws calls build_workspace directly: $direct"
[ "$(grep -c 'nros_ws_row "$record"' "$ws")" -ge 2 ] \
    || bad "E: $ws's row loops do not go through nros_ws_row"

# H — reach of the lanes.
nros_grep_q -F 'row_failed_script="$repo_root/scripts/build/fixture-row-failed.sh"' \
    scripts/build/zephyr-fixture-make-driver.sh \
    || bad "H: the Zephyr make driver's leaves do not end through fixture-row-failed.sh"
nros_grep_q -F 'nros_fixture_row "census-prepass image: $dir $image" census_image' \
    scripts/build/census-prepass.sh || bad "H: the census prepass's image takes are not rows"
nros_grep_q -F 'nros_fixture_row_failed "census-prepass leaf: $name"' \
    scripts/build/census-prepass.sh || bad "H: the census prepass's leaf censuses are not rows"
nros_grep_q -F 'nros_fixture_row_failed "build-test-fixtures stage: $stage' justfile \
    || bad "H: build-test-fixtures does not record a failed stage in the ledger"
nros_grep_q -F 'nros_fixture_build_ledger_lets_cells_run check-fixtures-stale' \
    scripts/check-fixtures-stale.sh || bad "H: the stale-fixture preflight ignores the build ledger"
wf=.github/workflows/platform-matrix.yml
[ "$(grep -c 'fixture-rows-keep-going.sh "${{ runner.temp }}/failed-rows-' "$wf")" = 3 ] \
    || bad "H: not every platform-matrix build (tier1, tier2, tier2-nightly) runs under the keep-going owner"
[ "$(grep -c 'NROS_FIXTURE_BUILD_LEDGER: ${{ runner.temp }}/failed-rows-' "$wf")" = 3 ] \
    || bad "H: not every platform-matrix cells step reads its build's ledger"
[ "$(grep -c "steps.build.outcome == 'success' || steps.build.outcome == 'failure'" "$wf")" = 3 ] \
    || bad "H: not every platform-matrix cells step runs after a failed build"

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "$GATE: OK — kept-going rows are recorded, -e holds inside them, the owner fails on any, both builders, the Zephyr leaves, the census prepass and failed stages take the ledger, and platform-matrix's cells run past a failed build."
