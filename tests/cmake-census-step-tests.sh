#!/usr/bin/env bash
# tests/cmake-census-step-tests.sh -- phase-530 W6 (RFC-0120 D7).
#
# WHAT THIS MEASURES
#
# A leaf sized from its census depends on the census's DIGEST, and a build edge
# (`nros_census_step`, cmake/NanoRosCensusStep.cmake) re-takes the census and
# refreshes that digest write-if-changed before ninja decides whether
# `build.ninja` is stale. This drives the REAL module through a real
# CMake + Ninja build, with a STAND-IN CLI whose `take --leaf` counts the
# `PUB` lines of the leaf's `src/main.c` (an entity per line) and rewrites the
# digest only when that count moved -- the contract the real verb keeps
# (`write_census_digest`, unit-tested in nros-cli-core). Counts are
# `-- Configuring done` lines in the configure log the probe appends to:
#
#   T1  clean configure + build     1 configure, 0 re-runs, the edge ran
#   T2  no-op build                 still 1
#   T3  body edit (same entities)   still 1, the program rebuilt
#   T4  add an entity               exactly 2 (ONE re-run), and that configure
#                                   saw the new count
#   T5  no-op after                 still 2
#   F   NROS_LEAF_CENSUS_HOST       no edge, no CLI call (the census producer)
#   R   REACH                       every census reader adds the step, and no
#                                   reader depends on the census FILE (which
#                                   re-ran on every re-take)
#
# PRECONDITIONS ARE HARD FAILURES (issue 1758): no cmake/ninja/cc, red.
#
# Usage: ./tests/cmake-census-step-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

STEP_MODULE="$PROJECT_ROOT/cmake/NanoRosCensusStep.cmake"

FAILURES=0
CHECKS=0
fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}
check() { CHECKS=$((CHECKS + 1)); }

for t in cmake ninja cc; do
    if ! command -v "$t" >/dev/null 2>&1; then
        fail "$t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done
if [ ! -f "$STEP_MODULE" ]; then
    fail "module not found: $STEP_MODULE"
    exit 1
fi

init_test_tmpdir "nros-census-step"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# The stand-in CLI: `path --leaf L --digest` and `take --leaf L`.
# ---------------------------------------------------------------------------
STANDIN="$TEST_TMPDIR/nros"
CALLS="$TEST_TMPDIR/calls.log"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "%s\n" "$*" >> "$STANDIN_LOG"' \
    'leaf="$6"' \
    'digest="$leaf/build/nros/census/leaf.census.digest"' \
    'case "$1 $2 $3 $4" in' \
    '  "dev image entity-census path") echo "$digest" ;;' \
    '  "dev image entity-census take")' \
    '     new="entities=$(grep -c PUB "$leaf/src/main.c")"' \
    '     mkdir -p "$(dirname "$digest")"' \
    '     [ -f "$digest" ] && [ "$(cat "$digest")" = "$new" ] && exit 0' \
    '     echo "$new" > "$digest" ;;' \
    'esac' > "$STANDIN"
chmod +x "$STANDIN"

LEAF="$TEST_TMPDIR/leaf"
BUILD="$LEAF/build/target"
CONFIGURES="$TEST_TMPDIR/configures.log"
mkdir -p "$LEAF/src"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(nros_census_step_probe C)' \
    "include(\"$STEP_MODULE\")" \
    "execute_process(COMMAND \"$STANDIN\" dev image entity-census take --leaf \"\${CMAKE_SOURCE_DIR}\")" \
    'nros_census_step(CLI "$ENV{NROS_CLI}" LEAF "${CMAKE_SOURCE_DIR}")' \
    'file(READ "${CMAKE_SOURCE_DIR}/build/nros/census/leaf.census.digest" _d)' \
    "file(APPEND \"$CONFIGURES\" \"\${_d}\")" \
    'add_executable(prog src/main.c)' > "$LEAF/CMakeLists.txt"
write_main() { # $1 = number of PUB lines, $2 = return value (the "body")
    {
        echo 'int main(void) {'
        for _ in $(seq 1 "$1"); do echo '    /* PUB */'; done
        echo "    return $2;"
        echo '}'
    } > "$LEAF/src/main.c"
}
write_main 1 0

export STANDIN_LOG="$CALLS" NROS_CLI="$STANDIN"
configures() { wc -l < "$CONFIGURES" 2>/dev/null || echo 0; }
build() { ninja -C "$BUILD" 2>&1; }

log_header "T1. clean configure + build"
: > "$CALLS"
cmake -S "$LEAF" -B "$BUILD" -G Ninja >/dev/null 2>&1 || fail "T1: configure failed"
OUT="$(build)"
check
if [ "$(configures)" -eq 1 ] && nros_grep_q 'census step' <<<"$OUT" \
    && ! nros_grep_q 'Re-running CMake' <<<"$OUT"; then
    log_success "1 configure, the edge ran, no re-run"
else
    fail "T1: configures=$(configures)
$OUT"
fi

log_header "T2. no-op build"
OUT="$(build)"
check
if [ "$(configures)" -eq 1 ] && ! nros_grep_q 'Re-running CMake' <<<"$OUT"; then
    log_success "still 1 configure"
else
    fail "T2: configures=$(configures)
$OUT"
fi

log_header "T3. body edit: no re-configure"
write_main 1 7
OUT="$(build)"
check
if [ "$(configures)" -eq 1 ] && ! nros_grep_q 'Re-running CMake' <<<"$OUT" \
    && nros_grep_q 'Building C object' <<<"$OUT"; then
    log_success "the program rebuilt; 0 re-configures"
else
    fail "T3: configures=$(configures)
$OUT"
fi

log_header "T4. add an entity: exactly one re-configure, which sees it"
write_main 2 7
OUT="$(build)"
check
if [ "$(configures)" -eq 2 ] && [ "$(grep -c 'Re-running CMake' <<<"$OUT")" -eq 1 ] \
    && [ "$(tail -n1 "$CONFIGURES")" = "entities=2" ]; then
    log_success "1 re-run; that configure read entities=2"
else
    fail "T4: configures=$(configures) last=$(tail -n1 "$CONFIGURES")
$OUT"
fi

log_header "T5. no-op after"
OUT="$(build)"
check
if [ "$(configures)" -eq 2 ] && ! nros_grep_q 'Re-running CMake' <<<"$OUT"; then
    log_success "still 2 configures"
else
    fail "T5: configures=$(configures)
$OUT"
fi

log_header "F. inside the census's host build: no step"
: > "$CALLS"
FDIR="$TEST_TMPDIR/f"
mkdir -p "$FDIR"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(nros_census_step_host NONE)' \
    "include(\"$STEP_MODULE\")" \
    'nros_census_step(CLI "$ENV{NROS_CLI}" LEAF "${CMAKE_SOURCE_DIR}")' \
    'if(TARGET nros_census_step)' \
    '    message(FATAL_ERROR "the census host build got a census step")' \
    'endif()' > "$FDIR/CMakeLists.txt"
RC=0
NROS_LEAF_CENSUS_HOST=1 cmake -S "$FDIR" -B "$FDIR/build" -G Ninja >/dev/null 2>&1 || RC=$?
check
if [ "$RC" -eq 0 ] && [ ! -s "$CALLS" ]; then
    log_success "NROS_LEAF_CENSUS_HOST: no step, no CLI call"
else
    fail "case F: rc=$RC calls: $(cat "$CALLS")"
fi

# ---------------------------------------------------------------------------
log_header "R. reach: every census reader adds the step; none watches the census file"
for f in cmake/NanoRosLeafEntityFacts.cmake zephyr/cmake/nros_cargo_build.cmake; do
    check
    if nros_grep_q 'nros_census_step(' <"$PROJECT_ROOT/$f"; then
        log_success "$f adds the census step"
    else
        fail "$f reads a leaf census and adds no census step (RFC-0120 D7)"
    fi
done
check
# A leaf reader that registers `entity-census path --leaf` (no --digest) as a
# configure dependency re-runs on every re-take, body edits included.
nros_git_grep plain -C "$PROJECT_ROOT" -nE 'entity-census path --leaf "[^"]*"\)?$' \
    -- 'cmake/*.cmake' 'zephyr/*.cmake'
if [ -z "$plain" ]; then
    log_success "no cmake reader asks for the census file's own path"
else
    fail "a reader depends on the census FILE, not its digest:
$plain"
fi
check
src="$PROJECT_ROOT/packages/cli/nros-cli-core/src/cmd/west_leaf_sizing.rs"
if nros_grep_q 'census_digest_path' <"$src"; then
    log_success "west-leaf-sizing names the census digest as its input"
else
    fail "west-leaf-sizing's input row names the census file, not its digest"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-census-step: $CHECKS checks held"
    exit 0
fi
log_error "cmake-census-step: $FAILURES of $CHECKS checks failed"
exit 1
