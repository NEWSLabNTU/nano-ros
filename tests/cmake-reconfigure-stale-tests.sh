#!/usr/bin/env bash
# Negative control for scripts/nros-reconfigure-stale.sh.
#
# The script reports "OK, N build dir(s) load" on a healthy tree, and that
# verdict is worth nothing on its own: a probe that can never fail prints the
# same thing as a probe that works. So this drives it against a build dir that
# is ACTUALLY wedged, in the way ninja actually wedges — a manifest error
# raised at LOAD, which is why ninja cannot re-run cmake to repair itself
# (issue 0882).
#
# ONE WEDGE IS NOT THE WHOLE RULE (issue 1406)
#
# Cases 1–4 all wedge the manifest so it cannot be PARSED, and every one of them
# passed on the day 38 manifests in this repo named a source deleted seventeen
# days earlier. Those manifests load; ninja does not stat an edge's inputs until
# it runs the edge. A selftest made only of unparseable manifests therefore
# leaves the gate in exactly the state that let this through, so cases 5+ cover
# the other shape: a manifest that LOADS and refers to something that is gone.
#
# Real cmake + real ninja, no compiler (`project(... NONE)`), ~2 s.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
source "$ROOT/scripts/lib/grep-q.sh"
SCRIPT="$ROOT/scripts/nros-reconfigure-stale.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }

# ---------------------------------------------------------------- fixture
mkdir -p "$TMP/src"
cat > "$TMP/src/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.16)
project(reconfigure_stale_probe NONE)
file(WRITE "${CMAKE_CURRENT_BINARY_DIR}/marker.txt" "configured\n")
add_custom_target(noop COMMAND ${CMAKE_COMMAND} -E true)
EOF

BUILD="$TMP/build"
cmake -S "$TMP/src" -B "$BUILD" -G Ninja >/dev/null 2>&1 \
    || fail "could not configure the probe project"

# ------------------------------------------------- 1: healthy dir passes
if ! "$SCRIPT" --check "$TMP" >/dev/null 2>&1; then
    fail "a freshly configured build dir was reported as wedged"
fi
echo "  ok: a healthy build dir loads"

# ------------------------------------------------- 2: wedged dir is caught
# Two build statements producing one output — `multiple rules generate`, raised
# at LOAD. This is the shape that made the dir unrecoverable by ninja itself.
cat >> "$BUILD/build.ninja" <<'EOF'

rule nros_probe_dup
  command = true
build dup_output.txt: nros_probe_dup
build dup_output.txt: nros_probe_dup
EOF

if ninja -C "$BUILD" -t targets >/dev/null 2>&1; then
    fail "the manifest still loads — the wedge did not take, so the rest of \
this test would pass vacuously"
fi
echo "  ok: the wedge makes the manifest unloadable"

if "$SCRIPT" --check "$TMP" >/dev/null 2>&1; then
    fail "--check reported a wedged build dir as healthy"
fi
echo "  ok: --check detects the wedged dir"

# Capture first, then grep: the script exits non-zero by design here, and under
# `pipefail` that failure would propagate through the pipeline and be read as
# "grep found nothing".
check_out="$("$SCRIPT" --check "$TMP" 2>&1 || true)"
nros_grep_q "$BUILD" <<<"$check_out" || fail "--check did not name the offending dir"
echo "  ok: --check names the dir"

# ------------------------------------------------- 3: repair fixes in place
touch "$BUILD/marker.txt"
before="$(find "$BUILD" -name CMakeCache.txt | wc -l)"
"$SCRIPT" "$TMP" >/dev/null 2>&1 || fail "repair reported failure"

if ! ninja -C "$BUILD" -t targets >/dev/null 2>&1; then
    fail "the manifest still does not load after repair"
fi
echo "  ok: re-configure repairs the manifest in place"

after="$(find "$BUILD" -name CMakeCache.txt | wc -l)"
[ "$before" = "$after" ] || fail "repair did not preserve the build dir"
[ -f "$BUILD/marker.txt" ] || fail "repair deleted build-dir contents — it must \
never wipe, that is the whole point"
echo "  ok: repair preserved the build dir (no wipe)"

# ------------------------------------ 4: no CMakeCache => reported, not wiped
NOCACHE="$TMP/owned"
mkdir -p "$NOCACHE"
printf 'this is not a valid manifest\n' > "$NOCACHE/build.ninja"
out="$("$SCRIPT" "$TMP" 2>&1 || true)"
nros_grep_q "no CMakeCache.txt" <<<"$out" \
    || fail "a build dir with no CMakeCache was not reported as unrepairable"
[ -f "$NOCACHE/build.ninja" ] \
    || fail "the unrepairable dir was deleted — it must be left as evidence"
echo "  ok: an unrepairable dir is reported and left in place"

# ============================================================ issue 1406
# A manifest that LOADS and names a file that is gone. Its own tree, because
# `$TMP` now holds the deliberately-unrepairable dir from case 4 and every
# verdict below has to be about the dir under test.
REFS="$TMP/refs"
mkdir -p "$REFS/src/inputs"
cat > "$REFS/src/CMakeLists.txt" <<'EOF'
cmake_minimum_required(VERSION 3.16)
project(reconfigure_stale_refs NONE)
# A GLOB, so that deleting a source and re-configuring actually changes the
# generated manifest — which is what makes `cmake <build-dir>` the repair.
file(GLOB PROBE_INPUTS "${CMAKE_CURRENT_SOURCE_DIR}/inputs/*.txt")
add_custom_command(
    OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/gen.txt"
    COMMAND ${CMAKE_COMMAND} -E touch "${CMAKE_CURRENT_BINARY_DIR}/gen.txt"
    DEPENDS ${PROBE_INPUTS})
# `gen.txt` is an input HERE and an output ABOVE. A probe that only stats paths
# would call it missing until the build runs; it must not.
add_custom_command(
    OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/second.txt"
    COMMAND ${CMAKE_COMMAND} -E touch "${CMAKE_CURRENT_BINARY_DIR}/second.txt"
    DEPENDS "${CMAKE_CURRENT_BINARY_DIR}/gen.txt")
add_custom_target(probe ALL DEPENDS "${CMAKE_CURRENT_BINARY_DIR}/second.txt")
EOF
echo alive > "$REFS/src/inputs/present.txt"
echo doomed > "$REFS/src/inputs/doomed.txt"

RBUILD="$REFS/build"
cmake -S "$REFS/src" -B "$RBUILD" -G Ninja >/dev/null 2>&1 \
    || fail "could not configure the reference probe project"

# ---------------------- 5: a configured dir with every input present is clean
# The negative control for the false-positive direction. A cmake manifest is
# mostly generated files that do not exist yet (`gen.txt`, `second.txt`,
# `CMakeFiles/*`), so a probe that merely stats every input would report this
# healthy dir, and cases 6-8 would pass while meaning nothing.
"$SCRIPT" --check "$REFS" >/dev/null 2>&1 \
    || fail "a freshly configured dir was reported stale — the probe is \
reporting pending build outputs as missing inputs"
echo "  ok: pending build outputs are not reported as missing"

# ---------------------- 6: delete a source; the manifest still LOADS
rm "$REFS/src/inputs/doomed.txt"
ninja -C "$RBUILD" -t targets >/dev/null 2>&1 \
    || fail "the manifest stopped loading — then this case is testing case 2 \
again, and the 1406 shape is still uncovered"
echo "  ok: a manifest naming a deleted source still loads"

if "$SCRIPT" --check "$REFS" >/dev/null 2>&1; then
    fail "--check reported a dir naming a deleted source as healthy — this is \
issue 1406 exactly"
fi
echo "  ok: --check detects the deleted source"

refs_out="$("$SCRIPT" --check "$REFS" 2>&1 || true)"
nros_grep_q "doomed.txt" <<<"$refs_out" \
    || fail "--check did not name the missing file; the count alone is what \
sent the last reader 400 lines into a fixture-build log"
nros_grep_q "$RBUILD" <<<"$refs_out" || fail "--check did not name the dir"
echo "  ok: --check names both the dir and the missing path"

# ---------------------- 7: re-configure repairs it, in place
"$SCRIPT" "$REFS" >/dev/null 2>&1 || fail "repair of the stale-reference dir failed"
"$SCRIPT" --check "$REFS" >/dev/null 2>&1 \
    || fail "the dir is still stale after re-configure"
[ -f "$RBUILD/CMakeCache.txt" ] || fail "repair did not preserve the build dir"
echo "  ok: re-configure drops the dead reference (no wipe)"

# ---------------------- 8: the CMakeCache half of the same blind spot
# The first sighting was a cache holding a deleted `CMAKE_MAKE_PROGRAM`, which
# the ninja probe cannot see either: the manifest loads and the cache is not a
# manifest. Same probe, so it cannot be fixed for one fact and not the other.
gone_tool="$TMP/no-such-dir/ninja"
printf 'NROS_PROBE_TOOL:FILEPATH=%s\n' "$gone_tool" >> "$RBUILD/CMakeCache.txt"
ninja -C "$RBUILD" -t targets >/dev/null 2>&1 \
    || fail "appending a cache line broke the manifest — wrong thing under test"
if "$SCRIPT" --check "$REFS" >/dev/null 2>&1; then
    fail "--check reported a cache naming a deleted tool as healthy"
fi
cache_out="$("$SCRIPT" --check "$REFS" 2>&1 || true)"
nros_grep_q "NROS_PROBE_TOOL" <<<"$cache_out" \
    || fail "--check did not name the cache variable holding the dead path"
echo "  ok: --check detects a cache entry pointing at a deleted tool"

echo "cmake-reconfigure-stale-tests: OK (13 case(s))"
