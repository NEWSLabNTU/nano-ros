#!/usr/bin/env bash
# phase-319 W3 (issue 0351) — print a compile-check fixture id when its
# build-input signature is missing or stale.
#
# The sibling of `workspace-fixture-stale.sh`, for the lane that never had one.
# `.compile-ok` answers "did a build succeed at some point?"; comparing
# `.inputsig` against a freshly computed signature answers "was this built from
# the sources on disk right now?" — which is the question the gate is for.
#
# Usage: compile-check-stale.sh <manifest-record>
set -u

line="$1"
IFS=$'\x1f' read -r id builder dir _pkg _mdir _target _profiles _output <<< "$line"
[ -n "${id:-}" ] && [ -n "${builder:-}" ] || exit 0

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

# cmake rows stamp under build/cmake-fixtures/<id>; everything else under
# build/compile-check/<id> (the same roots the resolvers use).
if [ "$builder" = "cmake-configure" ]; then
    stamp="$repo_root/build/cmake-fixtures/$id/.inputsig"
else
    stamp="$repo_root/build/compile-check/$id/.inputsig"
fi

expected="$(bash "$repo_root/scripts/build/compile-check-signature.sh" "$line" 2>/dev/null)" || {
    echo "$id (signature failed)"
    exit 0
}
actual="$(cat "$stamp" 2>/dev/null || true)"

if [ -z "$actual" ]; then
    echo "$id (missing $stamp)"
elif [ "$actual" != "$expected" ]; then
    echo "$id (stale $stamp)"
fi
