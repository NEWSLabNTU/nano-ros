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

# cmake rows stamp under `cmake-fixtures/<id>`; everything else under
# `compile-check/<id>`. RFC-0070 R3 (phase-334 W2.b step 2) — "the same roots the
# resolvers use" is now a fact rather than a comment: this probe, the writer
# (`scripts/build/compile-check-fixtures.sh`) and the Rust resolver all derive
# the root, so the probe cannot end up inspecting a tree the build did not write.
# `NROS_REPO_ROOT` is pinned to this script's own repo root — see the note in
# compile-check-fixtures.sh.
NROS_REPO_ROOT="$repo_root"
# shellcheck source=scripts/build/build-root.sh
source "$repo_root/scripts/build/build-root.sh"

if [ "$builder" = "cmake-configure" ]; then
    stamp="$(nros_build_dir "$NROS_KIND_CMAKE_FIXTURES" "$id")/.inputsig"
else
    stamp="$(nros_build_dir "$NROS_KIND_COMPILE_CHECK" "$id")/.inputsig"
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
