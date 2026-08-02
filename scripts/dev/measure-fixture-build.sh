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

built="$(grep -c '^ *built: ' "$LOG" || true)"
errors="$(grep -ciE '^(error|FAILED)' "$LOG" || true)"

cat <<REPORT

\`\`\`
just build-test-fixtures lane=$LANE   (manifest-declared build trees wiped first)
  BUILD_EXIT      $rc
  WALL_SECONDS    $wall   ($((wall / 3600))h $(((wall % 3600) / 60))m)
  fixtures built  $built
  errors          $errors
  log             $LOG
\`\`\`
REPORT

exit "$rc"
