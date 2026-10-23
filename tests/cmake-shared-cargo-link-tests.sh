#!/bin/bash
# tests/cmake-shared-cargo-link-tests.sh -- issue 1990
#
# `cmake/NanoRosSharedCargoLink.cmake` runs at build time, before every cargo
# edge, and re-creates the directory a build dir's `cargo` link names when the
# runner sweep removed it after the configure. Asserted with `cmake -P`:
#
#   1. a DANGLING link -> the target is created; `mkdir` through the link then
#      works (it is EEXIST before — the failure the issue measured);
#   2. a link whose target exists -> left alone, contents kept;
#   3. a real directory (the degrade path, no sharing) -> left alone;
#   4. no `cargo` at all (sharing off) -> nothing created;
#   5. a RELATIVE link target -> resolved against the link's own directory;
#   6. a link to a regular FILE -> refused, the file kept.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$ROOT/cmake/NanoRosSharedCargoLink.cmake"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

run() { cmake "-DNROS_SHARED_CARGO_LINK=$1" -P "$SCRIPT"; }
bad() { echo "  FAIL: $*"; fail=1; }

# 1 -- dangling link.
mkdir -p "$WORK/1/build"
ln -s "$WORK/1/shared/key" "$WORK/1/build/cargo"
if mkdir -p "$WORK/1/build/cargo/x" 2>/dev/null; then
    bad "case 1 precondition: mkdir through a dangling link succeeded"
fi
out="$(run "$WORK/1/build/cargo" 2>&1)" || bad "case 1: script failed: $out"
[ -d "$WORK/1/shared/key" ] || bad "case 1: target not re-created"
[ -L "$WORK/1/build/cargo" ] || bad "case 1: link no longer a symlink"
[[ "$out" == *'issue 1990'* ]] || bad "case 1: no message saying what it re-created"
mkdir -p "$WORK/1/build/cargo/x" || bad "case 1: mkdir through the link still fails"

# 2 -- live link: untouched.
mkdir -p "$WORK/2/build" "$WORK/2/shared/key"
echo keep > "$WORK/2/shared/key/artifact"
ln -s "$WORK/2/shared/key" "$WORK/2/build/cargo"
out="$(run "$WORK/2/build/cargo" 2>&1)" || bad "case 2: script failed: $out"
[ "$(cat "$WORK/2/shared/key/artifact")" = keep ] || bad "case 2: contents changed"
[ -z "$out" ] || bad "case 2: printed on a live link: $out"

# 3 -- a real directory.
mkdir -p "$WORK/3/build/cargo"
out="$(run "$WORK/3/build/cargo" 2>&1)" || bad "case 3: script failed: $out"
[ -d "$WORK/3/build/cargo" ] && [ ! -L "$WORK/3/build/cargo" ] || bad "case 3: real dir changed"

# 4 -- nothing there.
mkdir -p "$WORK/4/build"
out="$(run "$WORK/4/build/cargo" 2>&1)" || bad "case 4: script failed: $out"
[ ! -e "$WORK/4/build/cargo" ] || bad "case 4: created something with sharing off"

# 5 -- relative target.
mkdir -p "$WORK/5/build"
ln -s ../shared/key "$WORK/5/build/cargo"
out="$(run "$WORK/5/build/cargo" 2>&1)" || bad "case 5: script failed: $out"
[ -d "$WORK/5/shared/key" ] || bad "case 5: relative target not re-created beside the link"

# 6 -- link to a file.
mkdir -p "$WORK/6/build" "$WORK/6/shared"
echo data > "$WORK/6/shared/key"
ln -s "$WORK/6/shared/key" "$WORK/6/build/cargo"
if out="$(run "$WORK/6/build/cargo" 2>&1)"; then
    bad "case 6: a link to a file passed"
fi
[ "$(cat "$WORK/6/shared/key")" = data ] || bad "case 6: the file was touched"

[ "$fail" -eq 0 ] && echo "cmake-shared-cargo-link: OK (6 cases)" || { echo "cmake-shared-cargo-link: FAILED"; exit 1; }
