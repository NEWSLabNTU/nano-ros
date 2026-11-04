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
#   M   Unix Makefiles (R40)        the build that changed the digest FAILS
#                                   ("entities changed; build again") before
#                                   the program links; the next one
#                                   re-configures once and links the new count
#   W   two census leaves (R41)     each gets its own step; an entity added to
#                                   the SECOND re-configures once
#   Z   called before `project()`   (the Zephyr road) the step still lands
#   C   the workspace road (R42)    the step keyed by a MODEL: a body edit
#                                   builds green with 0 re-runs, an added
#                                   entity re-runs once and is read
#   C2  the same under Makefiles    the digest-moving build fails (R40)
#   H   NROS_LEAF_CENSUS_HOST       the census host build locates no configure
#                                   seed (an unconfigured host sibling FATALed)
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

for t in cmake ninja make cc; do
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
    'leaf=""; model=""; fail_if_changed=""; prev=""' \
    'for a in "$@"; do' \
    '  [ "$prev" = --leaf ] && leaf="$a"' \
    '  [ "$prev" = --model ] && model="$a"' \
    '  [ "$a" = --fail-if-changed ] && fail_if_changed=1' \
    '  prev="$a"' \
    'done' \
    'digest="$leaf/build/nros/census/leaf.census.digest"; src="$leaf/src/main.c"' \
    'if [ -n "$model" ]; then digest="${model%.yaml}.census.digest"; src="$(dirname "$model")/src/main.c"; fi' \
    'case "$1 $2 $3 $4" in' \
    '  "dev image entity-census path") echo "$digest" ;;' \
    '  "dev image entity-census take")' \
    '     new="entities=$(grep -c PUB "$src")"' \
    '     mkdir -p "$(dirname "$digest")"' \
    '     [ -f "$digest" ] && [ "$(cat "$digest")" = "$new" ] && exit 0' \
    '     echo "$new" > "$digest"' \
    '     if [ -n "$fail_if_changed" ]; then' \
    '       echo "entities changed; build again" >&2; exit 1' \
    '     fi ;;' \
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
    'cmake_language(DEFER CALL _probe_no_step)' \
    'function(_probe_no_step)' \
    '    get_property(_t DIRECTORY "${CMAKE_SOURCE_DIR}" PROPERTY BUILDSYSTEM_TARGETS)' \
    '    if(_t MATCHES "nros_census_step")' \
    '        message(FATAL_ERROR "the census host build got a census step")' \
    '    endif()' \
    'endfunction()' > "$FDIR/CMakeLists.txt"
RC=0
NROS_LEAF_CENSUS_HOST=1 cmake -S "$FDIR" -B "$FDIR/build" -G Ninja >/dev/null 2>&1 || RC=$?
check
if [ "$RC" -eq 0 ] && [ ! -s "$CALLS" ]; then
    log_success "NROS_LEAF_CENSUS_HOST: no step, no CLI call"
else
    fail "case F: rc=$RC calls: $(cat "$CALLS")"
fi

# ---------------------------------------------------------------------------
# make_leaf <dir> <pubs> [before-project]: a leaf calling the step, logging
# each configure's digest to <dir>/configures.log. With before-project the
# step is called before `project()`, as the Zephyr hook does.
make_leaf() {
    local dir="$1" pubs="$2" early="${3:-}"
    mkdir -p "$dir/src"
    local step="nros_census_step(CLI \"\$ENV{NROS_CLI}\" LEAF \"\${CMAKE_CURRENT_SOURCE_DIR}\")"
    {
        echo 'cmake_minimum_required(VERSION 3.22)'
        if [ -n "$early" ]; then
            echo "include(\"$STEP_MODULE\")"
            echo "execute_process(COMMAND \"$STANDIN\" dev image entity-census take --leaf \"\${CMAKE_CURRENT_SOURCE_DIR}\")"
            echo "$step"
            echo 'project(nros_census_step_early C)'
        else
            echo 'project(nros_census_step_probe C)'
            echo "include(\"$STEP_MODULE\")"
            echo "execute_process(COMMAND \"$STANDIN\" dev image entity-census take --leaf \"\${CMAKE_CURRENT_SOURCE_DIR}\")"
            echo "$step"
        fi
        echo 'file(READ "${CMAKE_CURRENT_SOURCE_DIR}/build/nros/census/leaf.census.digest" _d)'
        echo 'file(APPEND "${CMAKE_CURRENT_SOURCE_DIR}/configures.log" "${_d}")'
        echo "add_executable(prog_$(basename "$dir") src/main.c)"
    } > "$dir/CMakeLists.txt"
    set_pubs "$dir" "$pubs" 0
}
set_pubs() { # <dir> <pubs> <body>
    {
        echo 'int main(void) {'
        for _ in $(seq 1 "$2"); do echo '    /* PUB */'; done
        echo "    return $3;"
        echo '}'
    } > "$1/src/main.c"
}
nconf() { wc -l < "$1/configures.log" 2>/dev/null || echo 0; }

log_header "M. Unix Makefiles: the digest-changing build fails before linking (R40)"
MDIR="$TEST_TMPDIR/make"
make_leaf "$MDIR" 1
RC=0
cmake -S "$MDIR" -B "$MDIR/build/t" -G "Unix Makefiles" >/dev/null 2>&1 || RC=$?
make -C "$MDIR/build/t" >/dev/null 2>&1 || RC=$?
set_pubs "$MDIR" 2 0
FIRST_RC=0
FIRST="$(make -C "$MDIR/build/t" 2>&1)" || FIRST_RC=$?
SECOND_RC=0
SECOND="$(make -C "$MDIR/build/t" 2>&1)" || SECOND_RC=$?
check
if [ "$RC" -eq 0 ] && [ "$FIRST_RC" -ne 0 ] \
    && nros_grep_q 'entities changed; build again' <<<"$FIRST" \
    && ! nros_grep_q 'Linking C executable' <<<"$FIRST" \
    && [ "$SECOND_RC" -eq 0 ] && [ "$(nconf "$MDIR")" -eq 2 ] \
    && [ "$(tail -n1 "$MDIR/configures.log")" = "entities=2" ]; then
    log_success "first make refused before linking; the next re-configured once with entities=2"
else
    fail "case M: rc=$RC first=$FIRST_RC second=$SECOND_RC configures=$(nconf "$MDIR")
--- first
$FIRST
--- second
$SECOND"
fi

log_header "W. two census leaves in one tree: each has its own step (R41)"
WDIR="$TEST_TMPDIR/two"
mkdir -p "$WDIR"
make_leaf "$WDIR/a" 1
make_leaf "$WDIR/b" 1
sed -i 's/^project(.*)$//; s/^cmake_minimum_required.*$//' "$WDIR/a/CMakeLists.txt" "$WDIR/b/CMakeLists.txt"
printf '%s\n' 'cmake_minimum_required(VERSION 3.22)' 'project(two C)' \
    'add_subdirectory(a)' 'add_subdirectory(b)' > "$WDIR/CMakeLists.txt"
RC=0
cmake -S "$WDIR" -B "$WDIR/build" -G Ninja >/dev/null 2>&1 || RC=$?
OUT="$(ninja -C "$WDIR/build" 2>&1)" || RC=$?
set_pubs "$WDIR/b" 2 0
OUT2="$(ninja -C "$WDIR/build" 2>&1)" || RC=$?
check
if [ "$RC" -eq 0 ] && [ "$(grep -c 'census step' <<<"$OUT")" -ge 2 ] \
    && [ "$(grep -c 'Re-running CMake' <<<"$OUT2")" -eq 1 ] \
    && [ "$(tail -n1 "$WDIR/b/configures.log")" = "entities=2" ]; then
    log_success "two steps; leaf b's added entity re-configured once"
else
    fail "case W: rc=$RC
$OUT
---
$OUT2"
fi

log_header "Z. the step called before project() (the Zephyr road)"
ZDIR="$TEST_TMPDIR/early"
make_leaf "$ZDIR" 1 early
RC=0
cmake -S "$ZDIR" -B "$ZDIR/build/t" -G Ninja >/dev/null 2>&1 || RC=$?
OUT="$(ninja -C "$ZDIR/build/t" 2>&1)" || RC=$?
set_pubs "$ZDIR" 2 0
OUT2="$(ninja -C "$ZDIR/build/t" 2>&1)" || RC=$?
check
if [ "$RC" -eq 0 ] && nros_grep_q 'census step' <<<"$OUT" \
    && [ "$(grep -c 'Re-running CMake' <<<"$OUT2")" -eq 1 ] \
    && [ "$(tail -n1 "$ZDIR/configures.log")" = "entities=2" ]; then
    log_success "deferred step landed; an added entity re-configured once"
else
    fail "case Z: rc=$RC
$OUT
---
$OUT2"
fi

# make_ws <dir>: the WORKSPACE road's shape -- the step keyed by a MODEL (the
# census beside it is taken from the host sibling), as NanoRosEntry calls it.
make_ws() {
    local dir="$1"
    mkdir -p "$dir/src"
    : > "$dir/m.yaml"
    printf '%s\n' \
        'cmake_minimum_required(VERSION 3.22)' \
        'project(wsroad C)' \
        "include(\"$STEP_MODULE\")" \
        "execute_process(COMMAND \"$STANDIN\" dev image entity-census take --model \"\${CMAKE_SOURCE_DIR}/m.yaml\")" \
        'nros_census_step(CLI "$ENV{NROS_CLI}" MODEL "${CMAKE_SOURCE_DIR}/m.yaml"' \
        '    WORKSPACE "${CMAKE_SOURCE_DIR}")' \
        'file(READ "${CMAKE_SOURCE_DIR}/m.census.digest" _d)' \
        'file(APPEND "${CMAKE_SOURCE_DIR}/configures.log" "${_d}")' \
        'add_executable(cross_entry src/main.c)' > "$dir/CMakeLists.txt"
    set_pubs "$dir" 1 0
}

log_header "C. the workspace road re-takes its census during the build (R42)"
CDIR="$TEST_TMPDIR/wsroad"
make_ws "$CDIR"
RC=0
cmake -S "$CDIR" -B "$CDIR/build" -G Ninja >/dev/null 2>&1 || RC=$?
OUT="$(ninja -C "$CDIR/build" 2>&1)" || RC=$?
set_pubs "$CDIR" 1 9
BODY_RC=0
BODY="$(ninja -C "$CDIR/build" 2>&1)" || BODY_RC=$?
set_pubs "$CDIR" 2 9
ADD_RC=0
ADD="$(ninja -C "$CDIR/build" 2>&1)" || ADD_RC=$?
check
if [ "$RC" -eq 0 ] && [ "$BODY_RC" -eq 0 ] \
    && ! nros_grep_q 'Re-running CMake' <<<"$BODY" \
    && nros_grep_q 'Linking C executable' <<<"$BODY" \
    && [ "$ADD_RC" -eq 0 ] && [ "$(grep -c 'Re-running CMake' <<<"$ADD")" -eq 1 ] \
    && [ "$(nconf "$CDIR")" -eq 2 ] \
    && [ "$(tail -n1 "$CDIR/configures.log")" = "entities=2" ] \
    && nros_grep_q -- '--model' <"$CALLS"; then
    log_success "body edit: green, 0 re-runs; added entity: 1 re-run that read entities=2"
else
    fail "case C: rc=$RC body=$BODY_RC add=$ADD_RC configures=$(nconf "$CDIR")
$OUT
--- body
$BODY
--- add
$ADD"
fi

log_header "C2. the workspace road under Unix Makefiles (R40)"
C2DIR="$TEST_TMPDIR/wsmake"
make_ws "$C2DIR"
RC=0
cmake -S "$C2DIR" -B "$C2DIR/build" -G "Unix Makefiles" >/dev/null 2>&1 || RC=$?
make -C "$C2DIR/build" >/dev/null 2>&1 || RC=$?
set_pubs "$C2DIR" 2 0
FIRST_RC=0
FIRST="$(make -C "$C2DIR/build" 2>&1)" || FIRST_RC=$?
SECOND_RC=0
make -C "$C2DIR/build" >/dev/null 2>&1 || SECOND_RC=$?
check
if [ "$RC" -eq 0 ] && [ "$FIRST_RC" -ne 0 ] \
    && nros_grep_q 'entities changed; build again' <<<"$FIRST" \
    && ! nros_grep_q 'Linking C executable' <<<"$FIRST" \
    && [ "$SECOND_RC" -eq 0 ] && [ "$(tail -n1 "$C2DIR/configures.log")" = "entities=2" ]; then
    log_success "first make refused before linking; the next linked entities=2"
else
    fail "case C2: rc=$RC first=$FIRST_RC second=$SECOND_RC
$FIRST"
fi

log_header "H. the census's host build reads no configure seed (it is the producer)"
: > "$CALLS"
HDIR="$TEST_TMPDIR/hostseed"
mkdir -p "$HDIR"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(hostseed NONE)' \
    "include(\"$PROJECT_ROOT/cmake/NanoRosResolved.cmake\")" \
    'nros_resolved_dir(_d)' \
    'message(STATUS "PROBE_DIR=[${_d}]")' > "$HDIR/CMakeLists.txt"
RC=0
OUT="$(NROS_LEAF_CENSUS_HOST=1 cmake -S "$HDIR" -B "$HDIR/build" \
    -DNROS_CONFIGURED_PROJECT=/nonexistent -DNROS_CONFIGURED_IMAGE=b:linux 2>&1)" || RC=$?
check
if [ "$RC" -eq 0 ] && nros_grep_q 'PROBE_DIR=\[\]' <<<"$OUT" && ! nros_grep_q '^locate' <"$CALLS"; then
    log_success "NROS_LEAF_CENSUS_HOST: no locate, no seed"
else
    fail "case H: rc=$RC calls: $(cat "$CALLS")
$OUT"
fi

# ---------------------------------------------------------------------------
log_header "R. reach: every census reader adds the step; none watches the census file"
check
if nros_grep_q 'nros_census_step(' <"$PROJECT_ROOT/cmake/NanoRosEntry.cmake" \
    && ! nros_grep_q 'CMAKE_CONFIGURE_DEPENDS "${_rfc_census}"' <"$PROJECT_ROOT/cmake/NanoRosEntry.cmake"; then
    log_success "cmake/NanoRosEntry.cmake adds the census step and does not watch the census file (R42)"
else
    fail "cmake/NanoRosEntry.cmake does not re-take its census during a build, or watches the census FILE (R42)"
fi
check
nros_git_grep second -C "$PROJECT_ROOT" -n 'nros_census_check_step' -- 'cmake/*.cmake' 'zephyr/*.cmake'
if [ -z "$second" ]; then
    log_success "one mechanism: no read-only census check step remains"
else
    fail "a second census mechanism remains:
$second"
fi
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
