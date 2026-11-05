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
# Usage:  bash scripts/dev/measure-fixture-build.sh [lane]     (default: linux)
#         bash scripts/dev/measure-fixture-build.sh --platform <token> [--no-sccache]
#
# `--platform` (phase-535 W3) scopes the measurement to ONE fixture platform
# token (`freertos`, `nuttx`, …): it wipes the artifact root of every row of
# that token, in both row tables (`fixtures-manifest.py coords`), and times
# `just <module> build-fixtures`, the module `PlatformId::just_module` names.
# No lane builds one platform, and a pilot that moves one platform's pairs
# needs a before/after over exactly the rows it touched plus the rows beside
# them. `--no-sccache` clears the rustc wrapper for the timed build, so a
# cached crate from the BEFORE run cannot shorten the AFTER one.
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

PLATFORM=""
NO_SCCACHE=0
LANE=""
while [ $# -gt 0 ]; do
    case "$1" in
        --platform) PLATFORM="${2:?--platform needs a token}"; shift 2 ;;
        --no-sccache) NO_SCCACHE=1; shift ;;
        -*) echo "error: unknown option $1" >&2; exit 2 ;;
        *) LANE="$1"; shift ;;
    esac
done
if [ -n "$PLATFORM" ] && [ -n "$LANE" ]; then
    echo "error: give a lane or --platform, not both" >&2
    exit 2
fi
[ -n "$PLATFORM" ] || LANE="${LANE:-linux}"
SCOPE="${LANE:-platform-$PLATFORM}"
LOG="tmp/measure-fixture-build-${SCOPE}.log"
mkdir -p tmp

# `nros` on PATH is what activation provides. `NROS_REPO_DIR` was this
# sentinel until RFC-0103 D6 (amended 2026-10-12) stopped activation exporting
# it, after which the check refused every activated shell — and passed one
# that merely inherited a root from ANOTHER checkout (issue 1840).
# host-tool-path-ok: the question IS "did you source activate.sh" (PATH).
if ! command -v nros >/dev/null 2>&1; then
    echo "error: source ./activate.sh first (the sweep contract — PATH wires nros)." >&2
    exit 2
fi

echo "== prerequisites (untimed) =="
just setup-cli
just setup-launch-resolve

if [ -n "$PLATFORM" ]; then
    # The module that owns the token: `PlatformId::just_module`'s answer.
    MODULE="$(case "$PLATFORM" in
            linux) echo linux ;; zephyr|zephyr-cortex-m|zephyr-nucleo-f767zi|fvp) echo zephyr ;;
            freertos|freertos-posix) echo freertos ;; nuttx|nuttx-riscv) echo nuttx ;;
            threadx-linux) echo threadx_linux ;; threadx-riscv64) echo threadx_riscv64 ;;
            baremetal) echo qemu ;; *) echo "" ;;
        esac)"
    if [ -z "$MODULE" ]; then
        echo "error: no just module for platform token '$PLATFORM'" >&2
        exit 2
    fi
    echo "== wiping the artifact roots of every '$PLATFORM' row =="
    wiped=0
    while IFS= read -r path; do
        [ -n "$path" ] && [ -d "$path" ] || continue
        rm -rf "$path"
        wiped=$((wiped + 1))
        echo "  rm $path"
    done < <(
        python3 scripts/build/fixtures-manifest.py coords \
        | awk -F'\037' -v p="$PLATFORM" '$2 == p && $7 != "" { print $7 }' | sort -u
    )
    # A shared cargo group (`build/cargo-fixtures/<platform>[-<sig>]`) is where a
    # shared platform's Rust rows really land; the row's artifact root above
    # names the leaf it would use unshared. Missing this left the Rust leaves
    # warm in the first phase-535 W3 measurement.
    # shellcheck source=scripts/build/fixtures-target-dir.sh
    . scripts/build/fixtures-target-dir.sh
    # The same holds for the platform's Corrosion cargo target
    # (`build/corrosion-cargo/<platform>`), which every CMake configure of the
    # platform shares: leaving it warm would time the C side alone.
    cargo_dirs=("$(nros_build_dir "$NROS_KIND_CORROSION_CARGO")/$PLATFORM")
    if nros_fixture_platform_is_shared "$PLATFORM"; then
        groups="$(nros_build_dir "$NROS_KIND_CARGO_FIXTURES")"
        cargo_dirs+=("$groups/$PLATFORM" "$groups/$PLATFORM"-[0-9]*)
    fi
    for path in "${cargo_dirs[@]}"; do
        [ -d "$path" ] || continue
        rm -rf "$path"
        wiped=$((wiped + 1))
        echo "  rm $path"
    done
    echo "  wiped $wiped tree(s)"
    wrapper=()
    [ "$NO_SCCACHE" = 0 ] || wrapper=(--set NROS_SCCACHE_REAL "")
    echo "== timed build: just $MODULE build-fixtures (sccache $([ "$NO_SCCACHE" = 1 ] && echo off || echo as configured)) =="
    load_before="$(cut -d' ' -f1-3 /proc/loadavg)"
    start="$(date +%s)"
    set +e
    # CPU seconds as well as wall: the wall clock of a shared host moves with
    # whatever else runs on it, and the build's own user+sys time does not.
    cpu_file="tmp/measure-fixture-build-${SCOPE}.cpu"
    /usr/bin/time -o "$cpu_file" -f "%U %S" \
        just "${wrapper[@]}" "$MODULE" build-fixtures >"$LOG" 2>&1
    rc=$?
    set -e
    end="$(date +%s)"
    wall=$((end - start))
    rows="$(python3 scripts/build/fixtures-manifest.py coords | awk -F'\037' -v p="$PLATFORM" '$2 == p' | wc -l)"
    cat <<REPORT

\`\`\`
just $MODULE build-fixtures   (artifact roots of every '$PLATFORM' row wiped first)
  BUILD_EXIT      $rc
  WALL_SECONDS    $wall   ($((wall / 3600))h $(((wall % 3600) / 60))m)
  CPU_SECONDS     $(awk '{printf "%.0f (user %.0f + sys %.0f)", $1 + $2, $1, $2}' "$cpu_file" 2>/dev/null || echo unknown)
  load before     $load_before
  rows ($PLATFORM) $rows
  sccache         $([ "$NO_SCCACHE" = 1 ] && echo off || echo as configured)
  log             $LOG
\`\`\`
REPORT
    exit "$rc"
fi

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
