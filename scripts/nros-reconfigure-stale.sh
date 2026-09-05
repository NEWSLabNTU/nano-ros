#!/usr/bin/env bash
# Find build dirs whose `build.ninja` no longer LOADS, and repair them in place
# by re-running cmake.
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
#   --check   report wedged build dirs and exit non-zero; repair nothing.
#             (For CI / a gate: "is any build dir wedged right now?")
#   dir ...   roots to scan. Default: the repo root.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

CHECK_ONLY=0
ROOTS=()
for arg in "$@"; do
    case "$arg" in
        --check) CHECK_ONLY=1 ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        -*) echo "unknown flag: $arg" >&2; exit 2 ;;
        *) ROOTS+=("$arg") ;;
    esac
done
[ ${#ROOTS[@]} -gt 0 ] || ROOTS=("$ROOT")

for t in cmake ninja; do
    command -v "$t" >/dev/null 2>&1 || { echo "nros-reconfigure-stale: missing $t" >&2; exit 2; }
done

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

wedged=() ; repaired=() ; unrepairable=()

for manifest in "${MANIFESTS[@]}"; do
    d="$(dirname "$manifest")"
    # Load-only probe: `-t targets` parses the manifest and runs no edge. Any
    # failure here is a LOAD failure, which is the class this script repairs.
    if ninja -C "$d" -t targets >/dev/null 2>&1; then
        continue
    fi
    wedged+=("$d")
    rel="${d#"$ROOT"/}"

    if [ ! -f "$d/CMakeCache.txt" ]; then
        # Nothing to re-run configure FROM. This is the shape a tool that owns
        # its own build dir produces (`nros build`); re-running cmake by hand
        # would configure a different project than the one that wrote it.
        unrepairable+=("$rel (no CMakeCache.txt — regenerate it with the tool that owns this dir)")
        continue
    fi

    if [ "$CHECK_ONLY" = 1 ]; then
        continue
    fi

    echo "nros-reconfigure-stale: re-configuring $rel"
    if ! cmake "$d" >/dev/null 2>&1; then
        unrepairable+=("$rel (cmake re-configure failed — run \`cmake $rel\` to see why)")
        continue
    fi
    if ninja -C "$d" -t targets >/dev/null 2>&1; then
        repaired+=("$rel")
    else
        unrepairable+=("$rel (still does not load after re-configure)")
    fi
done

if [ ${#wedged[@]} -eq 0 ]; then
    echo "nros-reconfigure-stale: OK (${#MANIFESTS[@]} build dir(s) load)"
    exit 0
fi

if [ "$CHECK_ONLY" = 1 ]; then
    echo "nros-reconfigure-stale: ${#wedged[@]} of ${#MANIFESTS[@]} build dir(s) do NOT load:" >&2
    for d in "${wedged[@]}"; do echo "  ${d#"$ROOT"/}" >&2; done
    echo >&2
    echo "  ninja raises a manifest error at LOAD, so it can never re-run cmake" >&2
    echo "  to fix itself. Repair them in place (no wipe):" >&2
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
    echo "  The dirs above were NOT deleted. A wedged build dir is the only" >&2
    echo "  reproduction of whatever wedged it — usually a missing dependency" >&2
    echo "  edge — and \`rm -rf\` trades that evidence for a green build." >&2
    exit 1
fi
