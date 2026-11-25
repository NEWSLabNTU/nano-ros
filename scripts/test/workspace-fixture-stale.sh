#!/usr/bin/env bash
# Print each workspace fixture id whose build-input signature is missing/stale.
#
#   workspace-fixture-stale.sh < records    # one manifest record per line
#
# B39 -- the records are signed in ONE `workspace-fixture-signature.sh --batch`
# pass, which computes what rows share (the tool fingerprints, each dir's
# source manifest) once. Its caller ran this script per row through `parallel`,
# re-deriving all of it every time; the per-row bytes are the same either way.
set -u

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root" || exit 2

records=()
while IFS= read -r line; do
    [ -n "$line" ] && records+=("$line")
done
[ ${#records[@]} -gt 0 ] || exit 0

# id -> stamp path, for the rows that have a stamp dir.
declare -A stamp_of=()
signable=()
for line in "${records[@]}"; do
    IFS=$'\x1f' read -r id lang dir _bringup _entry build_subdir target_dir _codegen_out _defs <<< "$line"
    [ -n "${id:-}" ] && [ -n "${dir:-}" ] || continue
    if [ "$lang" = "rust" ]; then
        stamp_dir="${target_dir:-target}"
    else
        stamp_dir="$build_subdir"
    fi
    if [ -z "$stamp_dir" ]; then
        echo "$id (missing stamp dir)"
        continue
    fi
    stamp_of["$id"]="$dir/$stamp_dir/.nros-workspace-fixture.$id.inputsig"
    signable+=("$line")
done
[ ${#signable[@]} -gt 0 ] || exit 0

# A row the batch could not sign prints no line (and the batch exits 2), so the
# rows missing from its output are the ones that failed.
declare -A expected=()
while IFS=$'\t' read -r id sig; do
    [ -n "$id" ] && expected["$id"]="$sig"
done < <(printf '%s\n' "${signable[@]}" \
    | bash scripts/build/workspace-fixture-signature.sh --batch 2>/dev/null)

for line in "${signable[@]}"; do
    id="${line%%$'\x1f'*}"
    stamp="${stamp_of[$id]}"
    if [ -z "${expected[$id]+set}" ]; then
        echo "$id (signature failed)"
        continue
    fi
    actual="$(cat "$stamp" 2>/dev/null || true)"
    if [ -z "$actual" ]; then
        echo "$id (missing $stamp)"
    elif [ "$actual" != "${expected[$id]}" ]; then
        echo "$id (stale $stamp)"
    fi
done
