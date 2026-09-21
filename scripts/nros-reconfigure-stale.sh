#!/usr/bin/env bash
# Find build dirs that are STALE — the manifest no longer loads, or it still
# refers to files that are gone — and repair them in place by re-running cmake.
#
# Why this exists
# ---------------
# A generated `build.ninja` can reach a state ninja cannot recover from on its
# own. `multiple rules generate <x>` — and every other manifest error — is
# raised at LOAD, before any rule runs, so the rule that would re-run cmake and
# rewrite the manifest never gets the chance (issue 0882 hit exactly this after
# a half-applied fix). The build is wedged, and every ninja invocation in that
# directory reports the same error regardless of what you ask it to do.
#
# TWO questions, not one (issue 1406)
# -----------------------------------
# "Does the manifest load?" and "will this build?" are different questions, and
# for two weeks this script only asked the first. Ninja does not stat an edge's
# inputs until it runs the edge, so a manifest naming a source retired on
# 2026-09-04 loads perfectly well — 38 of them did, while this script printed
# "OK (352 build dir(s) load)" and the native fixture build died on one of them
# as a `cc1plus: fatal error … No such file or directory`.
#
# So the probe now also asks whether what the dir REFERS TO still exists:
# `scripts/lib/ninja_stale_refs.py` walks the manifest's edge inputs (minus
# everything it declares as an output) and the `CMakeCache.txt` tool paths. The
# cache half is the SAME blind spot's first sighting, a `CMAKE_MAKE_PROGRAM`
# that had been deleted; one probe answers both rather than leaving the other
# where it was.
#
# The recovery is `cmake <build-dir>`: it re-runs configure from the cached
# settings and regenerates `build.ninja` in place. That is cheap, keeps the
# object tree, and is the documented escape hatch (CLAUDE.md, "When a GENERATED
# build file is itself the problem, re-configure — do not wipe").
#
# It is NOT `rm -rf`. Wiping proves only that a full build works, which was
# never in doubt, and it destroys the one reproduction that would have shown
# which dependency edge was missing. This script therefore never deletes
# anything: a directory it cannot repair is REPORTED, so the next person still
# has the evidence.
#
# Usage
#   scripts/nros-reconfigure-stale.sh [--check] [dir ...]
#
#   --check   report stale build dirs and exit non-zero; repair nothing.
#             (For CI / a gate: "is any build dir stale right now?")
#   dir ...   roots to scan. Default: the repo root.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CHECK_ONLY=0
ROOTS=()
for arg in "$@"; do
    case "$arg" in
        --check) CHECK_ONLY=1 ;;
        # The whole header block, which ends at the last `#` line before `set`.
        # A fixed line number silently truncated the help the first time the
        # header grew, which is this file's own subject one size down.
        -h|--help) sed -n '2,/^set -/p' "$0" | sed '$d'; exit 0 ;;
        -*) echo "unknown flag: $arg" >&2; exit 2 ;;
        *) ROOTS+=("$arg") ;;
    esac
done
[ ${#ROOTS[@]} -gt 0 ] || ROOTS=("$ROOT")

for t in cmake ninja python3; do
    command -v "$t" >/dev/null 2>&1 || { echo "nros-reconfigure-stale: missing $t" >&2; exit 2; }
done

REFS="$ROOT/scripts/lib/ninja_stale_refs.py"

# Everything the given dirs REFER TO that is not there, as `dir<TAB>kind<TAB>path`
# in REFS_OUT. One python process for the whole sweep: 350 dirs cost ~2 s,
# against ~6 s for the per-dir `ninja -t targets` spawns that answer the other
# question.
#
# Written to a global rather than to stdout on purpose. `set -e` kills the
# assignment on the probe's non-zero exit — which is its NORMAL exit when it
# finds something — so the status is taken, not inherited (issue 1249); and a
# `$(…)` or `<(…)` wrapper would put the failure handler in a SUBSHELL, where
# its `exit 2` reaches nobody and a crashed probe reads as "found nothing".
REFS_OUT=""
stale_refs_for() {
    local rc=0
    REFS_OUT="$(python3 "$REFS" "$@")" || rc=$?
    if [ "$rc" -gt 1 ]; then
        echo "nros-reconfigure-stale: reference probe failed (exit $rc)" >&2
        exit 2
    fi
}

# Enumerate build dirs. Pruned aggressively: these trees are large, and none of
# the pruned ones hold a cmake build dir we own.
mapfile -t MANIFESTS < <(
    find "${ROOTS[@]}" \
        \( -name .git -o -name target -o -name 'target-*' -o -name node_modules \) -prune -o \
        -name build.ninja -print 2>/dev/null | sort
)

if [ ${#MANIFESTS[@]} -eq 0 ]; then
    echo "nros-reconfigure-stale: no build.ninja found under ${ROOTS[*]}"
    exit 0
fi

DIRS=()
for manifest in "${MANIFESTS[@]}"; do DIRS+=("$(dirname "$manifest")"); done

# dir -> the references it names that are gone, one `kind<TAB>path` per line.
stale_refs_for "${DIRS[@]}"
declare -A BAD_REFS=()
while IFS=$'\t' read -r rdir rkind rpath; do
    [ -n "$rdir" ] || continue
    BAD_REFS["$rdir"]="${BAD_REFS["$rdir"]:-}${rkind}	${rpath}"$'\n'
done <<< "$REFS_OUT"

wedged=() ; repaired=() ; unrepairable=()
declare -A REASON=()

for d in "${DIRS[@]}"; do
    why=""
    # Question 1 — does it LOAD? `-t targets` parses the manifest and runs no
    # edge, so any failure here is a LOAD failure (issue 0882's class).
    if ! ninja -C "$d" -t targets >/dev/null 2>&1; then
        why="does not load"
    fi
    # Question 2 — do its REFERENCES exist? A manifest can load and still name a
    # deleted source, which is issue 1406 and is invisible to question 1.
    refs="${BAD_REFS["$d"]:-}"
    if [ -n "$refs" ]; then
        n="$(printf '%s' "$refs" | grep -c . || true)"
        why="${why:+$why, }names $n missing reference(s)"
    fi
    [ -n "$why" ] || continue

    wedged+=("$d")
    rel="${d#"$ROOT"/}"
    REASON["$d"]="$why"

    if [ ! -f "$d/CMakeCache.txt" ]; then
        # Nothing to re-run configure FROM. This is the shape a tool that owns
        # its own build dir produces (`nros build`); re-running cmake by hand
        # would configure a different project than the one that wrote it.
        unrepairable+=("$rel ($why; no CMakeCache.txt — regenerate it with the tool that owns this dir)")
        continue
    fi

    if [ "$CHECK_ONLY" = 1 ]; then
        continue
    fi

    echo "nros-reconfigure-stale: re-configuring $rel ($why)"
    if ! cmake "$d" >/dev/null 2>&1; then
        unrepairable+=("$rel (cmake re-configure failed — run \`cmake $rel\` to see why)")
        continue
    fi
    still=""
    ninja -C "$d" -t targets >/dev/null 2>&1 || still="still does not load"
    stale_refs_for "$d"
    if [ -n "$REFS_OUT" ]; then
        m="$(printf '%s' "$REFS_OUT" | grep -c . || true)"
        still="${still:+$still, }still names $m missing reference(s)"
    fi
    if [ -z "$still" ]; then
        repaired+=("$rel ($why)")
    else
        unrepairable+=("$rel ($still after re-configure)")
    fi
done

if [ ${#wedged[@]} -eq 0 ]; then
    echo "nros-reconfigure-stale: OK (${#MANIFESTS[@]} build dir(s) load, and every reference they name exists)"
    exit 0
fi

if [ "$CHECK_ONLY" = 1 ]; then
    echo "nros-reconfigure-stale: ${#wedged[@]} of ${#MANIFESTS[@]} build dir(s) are STALE:" >&2
    for d in "${wedged[@]}"; do
        echo "  ${d#"$ROOT"/} — ${REASON["$d"]}" >&2
        # Name the paths, not just the count: the cost of the old verdict was
        # that the diagnosis arrived as a compiler error inside a fixture build.
        printf '%s' "${BAD_REFS["$d"]:-}" | while IFS=$'\t' read -r k p; do
            [ -n "$k" ] || continue
            echo "        $k: ${p#"$ROOT"/}" >&2
        done
    done
    echo >&2
    echo "  A manifest error is raised at LOAD, so ninja can never re-run cmake" >&2
    echo "  to fix itself; a manifest that names a file that is gone loads fine" >&2
    echo "  and fails mid-build instead. Both are repaired in place (no wipe):" >&2
    echo >&2
    echo "      just reconfigure-stale" >&2
    exit 1
fi

echo
echo "nros-reconfigure-stale: ${#repaired[@]} repaired, ${#unrepairable[@]} left"
for d in "${repaired[@]}"; do echo "  repaired  $d"; done
for d in "${unrepairable[@]}"; do echo "  LEFT      $d"; done

if [ ${#unrepairable[@]} -gt 0 ]; then
    echo
    echo "  The dirs above were NOT deleted. A stale build dir is the only" >&2
    echo "  reproduction of whatever staled it — usually a missing dependency" >&2
    echo "  edge — and \`rm -rf\` trades that evidence for a green build." >&2
    exit 1
fi
