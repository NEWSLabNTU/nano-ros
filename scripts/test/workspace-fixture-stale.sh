#!/usr/bin/env bash
# Print a workspace fixture id when its build-input signature is missing/stale.
set -u

line="$1"
IFS=$'\x1f' read -r id lang dir _bringup _entry build_subdir target_dir _codegen_out _defs <<< "$line"
[ -n "${id:-}" ] && [ -n "${dir:-}" ] || exit 0

if [ "$lang" = "rust" ]; then
    stamp_dir="${target_dir:-target}"
else
    stamp_dir="$build_subdir"
fi
[ -n "$stamp_dir" ] || {
    echo "$id (missing stamp dir)"
    exit 0
}

stamp="$dir/$stamp_dir/.nros-workspace-fixture.$id.inputsig"
expected="$(bash scripts/build/workspace-fixture-signature.sh "$line" 2>/dev/null)" || {
    echo "$id (signature failed)"
    exit 0
}
actual="$(cat "$stamp" 2>/dev/null || true)"

if [ -z "$actual" ]; then
    echo "$id (missing $stamp)"
elif [ "$actual" != "$expected" ]; then
    echo "$id (stale $stamp)"
fi
