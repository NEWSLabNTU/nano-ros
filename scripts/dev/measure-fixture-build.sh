#!/usr/bin/env bash
#
# Cold-build wall clock for a fixture lane — the phase-331 W1/W5 measurement.
#
# W1 was run by hand and only its NUMBERS were written down, so W5 had to
# reconstruct the method from a prose bullet ("wipe the manifest-declared
# workspace build dirs first — derive them from `fixtures-manifest.py
# list-workspaces`, **not** a `build-workspace-fixtures` glob"). A measurement
# whose procedure lives in prose is not a measurement anyone can repeat, and the
# whole point of W1/W5 is comparing two runs.
#
# Usage:  bash scripts/dev/measure-fixture-build.sh [lane]     (default: native)
#
# Emits a markdown block on stdout, ready to paste into
# `docs/roadmap/data/phase-331-w<N>-baseline.md`, and leaves the full build log
# at `tmp/measure-fixture-build-<lane>.log`.
#
# What "cold" means here: every build tree the manifest declares is deleted, so
# no fixture is reused. The rest of the tree (the cargo target dir, the CLI) is
# NOT wiped — W1 did not wipe it either, and a run that also rebuilt the
# toolchain would measure something else. Prerequisites are rebuilt first,
# deliberately and OUTSIDE the timed section, because the stale-CLI guard
# refuses to auto-rebuild mid-build (issues 0363/0197) and a run that dies on it
# measures nothing.

set -euo pipefail
cd "$(dirname "$0")/../.."

LANE="${1:-native}"
LOG="tmp/measure-fixture-build-${LANE}.log"
mkdir -p tmp

if [ -z "${NROS_REPO_DIR:-}" ]; then
    echo "error: source ./activate.sh first (the sweep contract — PATH wires nros," \
         "play_launch_parser, zenohd)." >&2
    exit 2
fi

echo "== prerequisites (untimed) =="
just setup-cli
just setup-launch-resolve

echo "== wiping manifest-declared build trees =="
# Fields, from `list-workspaces`: 3=dir 6=build_subdir 7=target_dir 8=codegen_out.
# Derived from the manifest rather than globbed, because non-default
# `build_subdir` values exist (`-freertos`, `-cyclonedds`, `-safety-talker`, …)
# and a `build-workspace-fixtures*` glob both misses some and over-matches
# others.
wiped=0
while IFS= read -r path; do
    [ -n "$path" ] || continue
    [ -d "$path" ] || continue
    rm -rf "$path"
    wiped=$((wiped + 1))
    echo "  rm $path"
done < <(
    python3 scripts/build/fixtures-manifest.py list-workspaces 2>/dev/null \
    | awk -F'\037' '{
        if ($6 != "") print $3 "/" $6;
        if ($7 != "") print $3 "/" $7;
        if ($8 != "") print $3 "/" $8;
      }' | sort -u
)
echo "  wiped $wiped tree(s)"

echo "== timed build: just build-test-fixtures lane=$LANE =="
start="$(date +%s)"
set +e
just build-test-fixtures "lane=$LANE" >"$LOG" 2>&1
rc=$?
set -e
end="$(date +%s)"
wall=$((end - start))

# The counts live in the PER-STAGE logs, not the wrapper one: `build-test-fixtures`
# fans out into `tmp/build-test-fixtures-<stamp>/<stage>.log` and the wrapper only
# echoes stage banners. Grepping the wrapper reported "fixtures built 0" for a run
# that built 72 — a measurement that reads zero and exits 0 is worse than one that
# fails, so it counts the stage logs and refuses a zero.
run_dir="$(ls -dt tmp/build-test-fixtures-* 2>/dev/null | head -1)"
built=0
errors=0
if [ -n "$run_dir" ] && [ -d "$run_dir" ]; then
    built="$(cat "$run_dir"/*.log 2>/dev/null | grep -c '^ *built: ' || true)"
    errors="$(cat "$run_dir"/*.log 2>/dev/null | grep -ciE '^(error|FAILED)' || true)"
fi
if [ "$rc" = 0 ] && [ "$built" = 0 ]; then
    echo "warning: build exited 0 but no 'built:' lines were found under" \
         "${run_dir:-<no run dir>} — the count is unreliable, do not record it." >&2
fi

# Per-stage seconds, so a wall-clock delta can be attributed rather than guessed
# (W1 recorded the native stage separately and W5 needed the same breakdown).
stages=""
if [ -n "$run_dir" ] && [ -f "$run_dir/build-test-fixtures.joblog" ]; then
    stages="$(awk 'NR>1{printf "  %-14s %6s s  (status %s)\n", $1, $4, $5}' \
        "$run_dir/build-test-fixtures.joblog")"
fi

cat <<REPORT

\`\`\`
just build-test-fixtures lane=$LANE   (manifest-declared build trees wiped first)
  BUILD_EXIT      $rc
  WALL_SECONDS    $wall   ($((wall / 3600))h $(((wall % 3600) / 60))m)
  fixtures built  $built
  errors          $errors
  log             $LOG
  stages:
$stages
\`\`\`
REPORT

exit "$rc"
