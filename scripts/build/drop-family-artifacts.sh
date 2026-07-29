#!/usr/bin/env bash
# Drop one platform family's build artifacts — RFC-0061 / phase-318 W5.a.
#
# A full sweep needs ~800 GB and hit **11 MB free** twice on 2026-07-28, which
# ended that run more effectively than any test failure. The artifacts are
# reproducible; the RESULT is what needs keeping. So tier 3 can run
# build -> test -> drop per family instead of accumulating every family's output
# until the disk gives out.
#
# Usage:
#   drop-family-artifacts.sh <platform>            # dry run: list what WOULD go
#   drop-family-artifacts.sh <platform> --confirm  # actually delete
#
# DRY RUN BY DEFAULT, deliberately. This deletes build trees; a flag typo that
# silently wipes the wrong family costs a rebuild measured in hours. `--confirm`
# is cheap to type and impossible to hit by accident.
#
# Only ever removes directories the fixture MANIFEST names for that platform, so
# it cannot wander into a tree it does not own. It never touches `target/`,
# `build/`, or another platform's dirs.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

platform="${1:-}"
confirm="${2:-}"

if [ -z "$platform" ]; then
    echo "usage: drop-family-artifacts.sh <platform> [--confirm]" >&2
    exit 2
fi

mapfile -t dirs < <(
    {
        for lang in c cpp; do
            python3 scripts/build/fixtures-manifest.py list --for-probe \
                --lang "$lang" --platform "$platform" 2>/dev/null \
                | awk -F'\x1f' 'NF>1 && $1 != "" && $2 != "" {print $1"/"$2}'
        done
        python3 scripts/build/fixtures-manifest.py list-workspaces --for-probe \
            --platform "$platform" 2>/dev/null \
            | awk -F'\x1f' 'NF>1 && $1 != "" && $6 != "" {print $3"/"$6}'
    } | sort -u
)

if [ "${#dirs[@]}" -eq 0 ]; then
    echo "no manifest-declared build dirs for platform '$platform' — nothing to drop"
    exit 0
fi

total=0
present=()
for d in "${dirs[@]}"; do
    [ -d "$d" ] || continue
    present+=("$d")
    sz="$(du -sm "$d" 2>/dev/null | awk '{print $1}')" || sz=0
    total=$((total + sz))
done

if [ "${#present[@]}" -eq 0 ]; then
    echo "platform '$platform': all ${#dirs[@]} declared dirs already absent"
    exit 0
fi

printf 'platform %s: %d build dir(s), %d MB\n' "$platform" "${#present[@]}" "$total"
for d in "${present[@]}"; do
    printf '  %s\n' "$d"
done

if [ "$confirm" != "--confirm" ]; then
    echo
    echo "DRY RUN — nothing deleted. Re-run with --confirm to free ${total} MB."
    exit 0
fi

for d in "${present[@]}"; do
    rm -rf "$d"
done
printf 'dropped %d dir(s), freed ~%d MB\n' "${#present[@]}" "$total"
