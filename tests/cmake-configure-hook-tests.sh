#!/usr/bin/env bash
# tests/cmake-configure-hook-tests.sh -- phase-530 W5 (RFC-0120 D5, D8).
#
# WHAT THIS MEASURES
#
# Every HOOKED host configure (west, `find_package(nano_ros)`,
# `nano_ros_workspace()`, a bare `add_subdirectory(<nano-ros>)`) runs
# `nros configure` for its image before its first reader, through ONE helper,
# `nros_configure_hook` (cmake/NanoRosConfigureHook.cmake). This test drives the
# helper with a STAND-IN CLI that records every invocation ($NROS_CLI is the
# resolver's first rung), and asserts:
#
#   A  a leaf, no image named      leaf-system asked for the default image, then
#                                  `configure <leaf> --image <it>` ONCE, then
#                                  `locate configured`; the configure is CURRENT
#   B  the same configure calls    still ONE configure (the CMake roads reach
#      the hook three times        `find_package(nano_ros)` from every member)
#   C  an image named              no leaf-system call; that image is configured
#   D  PROJECT named               the project is configured, not DIR
#   I  ALL_IMAGES                  `configure <dir>` with no --image, nothing
#                                  located (a workspace root naming no image)
#   J  IMAGES a b                  each configured, none located
#   K  IMAGES one                  configured AND located
#   E  a guest (no system.toml)    no CLI call at all
#   F  NROS_LEAF_CENSUS_HOST set   no CLI call at all (configure's own producer)
#   L  locate reports STALE        not CURRENT, so readers stay unseeded (R33)
#   G  configure fails             the host configure FAILS, quoting it
#   M  re-configure, same dir      `nros configure` runs again (the guard is
#                                  per configure, never cached)
#   N  image order                 CONFIGURED_IMAGE > NROS_IMAGE > default
#   O  R31 walk-up                 only a generated app walks up to <ws>
#   P  R38 input edges             editing a digested launch file re-runs the
#                                  host configure and `nros configure`
#   H  REACH                       every hooked entry point calls the helper, and
#                                  nothing else under cmake/ or zephyr/ spells
#                                  the `configure` command
#
# PRECONDITIONS ARE HARD FAILURES (issue 1758): no cmake, no verdict, red.
#
# Usage: ./tests/cmake-configure-hook-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

HOOK_MODULE="$PROJECT_ROOT/cmake/NanoRosConfigureHook.cmake"

FAILURES=0
CHECKS=0
fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}
check() { CHECKS=$((CHECKS + 1)); }

if [ ! -f "$HOOK_MODULE" ]; then
    fail "module not found: $HOOK_MODULE"
    exit 1
fi
if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

init_test_tmpdir "nros-configure-hook"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# The stand-in CLI: logs argv, answers the three verbs the hook asks.
# STANDIN_FAIL=1 makes `configure` fail.
# ---------------------------------------------------------------------------
STANDIN="$TEST_TMPDIR/nros"
CALLS="$TEST_TMPDIR/calls.log"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "%s\n" "$*" >> "$STANDIN_LOG"' \
    'case "$1 $2 $3" in' \
    '  "dev image leaf-system") echo "NROS_LEAF_IMAGE=default_img" ;;' \
    '  "locate configured "*) echo "set(NROS_LOCATE_RESOLVED \"\")"' \
    '     if [ -n "${STANDIN_INPUTS:-}" ]; then echo "set(NROS_LOCATE_INPUTS \"$STANDIN_INPUTS\")"; fi' \
    '     if [ -n "${STANDIN_STALE:-}" ]; then echo "set(NROS_LOCATE_STALE \"edited\")";' \
    '     else echo "set(NROS_LOCATE_CURRENT TRUE)"; fi ;;' \
    '  *) if [ "$1" = configure ]; then' \
    '       if [ -n "${STANDIN_FAIL:-}" ]; then echo "configure: boom" >&2; exit 3; fi' \
    '       echo "configure: up to date (1 image(s))"' \
    '     fi ;;' \
    'esac' > "$STANDIN"
chmod +x "$STANDIN"

# $1 = case dir; the project calls the hook HOOK_CALLS times (default 1).
write_project() {
    local dir="$1"
    mkdir -p "$dir"
    printf '%s\n' \
        'cmake_minimum_required(VERSION 3.20)' \
        'project(nros_configure_hook_probe NONE)' \
        "include(\"$HOOK_MODULE\")" \
        'if(NOT DEFINED HOOK_CALLS)' \
        '    set(HOOK_CALLS 1)' \
        'endif()' \
        'set(_all "")' \
        'if(PROBE_ALL)' \
        '    set(_all ALL_IMAGES)' \
        'elseif(PROBE_IMAGES)' \
        '    set(_all IMAGES ${PROBE_IMAGES})' \
        'endif()' \
        'foreach(_i RANGE 1 ${HOOK_CALLS})' \
        '    nros_configure_hook(DIR "${CMAKE_SOURCE_DIR}" IMAGE "${PROBE_IMAGE}"' \
        '        PROJECT "${PROBE_PROJECT}" ${_all} ROAD "the probe")' \
        'endforeach()' \
        'get_property(_cur GLOBAL PROPERTY NROS_CONFIGURED_CURRENT)' \
        'message(STATUS "PROBE_CURRENT=${_cur}")' > "$dir/CMakeLists.txt"
}

# run_case <name> <with-system.toml:0|1> [cmake -D args...]; sets OUT, RC, LOG.
run_case() {
    local name="$1" leaf="$2"
    shift 2
    local src="$TEST_TMPDIR/$name"
    write_project "$src"
    if [ "$leaf" = 1 ]; then
        printf '[system]\nname = "probe"\n' > "$src/system.toml"
    fi
    : > "$CALLS"
    RC=0
    OUT="$(STANDIN_LOG="$CALLS" NROS_CLI="$STANDIN" \
        cmake -S "$src" -B "$src/build" "$@" 2>&1)" || RC=$?
    LOG="$(cat "$CALLS")"
    SRC_DIR="$src"
}

count() { grep -c -- "$1" <<<"$LOG" || true; }

# ---------------------------------------------------------------------------
log_header "A. a leaf with no image named: default image, one configure, current"
run_case a 1
check
if [ "$RC" -eq 0 ] \
    && [ "$(count '^dev image leaf-system')" -eq 1 ] \
    && [ "$(count "^configure $SRC_DIR --image default_img\$")" -eq 1 ] \
    && [ "$(count "^locate configured --project $SRC_DIR --image default_img")" -eq 1 ] \
    && nros_grep_q 'PROBE_CURRENT=TRUE' <<<"$OUT"; then
    log_success "leaf-system -> configure <leaf> --image default_img -> locate; CURRENT"
else
    fail "case A: rc=$RC calls:
$LOG
$OUT"
fi

log_header "B. three hook calls in one configure: still ONE configure"
run_case b 1 -DHOOK_CALLS=3
check
if [ "$RC" -eq 0 ] && [ "$(count '^configure ')" -eq 1 ]; then
    log_success "one configure per host configure"
else
    fail "case B: want 1 configure, calls:
$LOG"
fi

log_header "C. an image named: no default lookup"
run_case c 1 -DPROBE_IMAGE=named
check
if [ "$RC" -eq 0 ] && [ "$(count '^dev image')" -eq 0 ] \
    && [ "$(count "^configure $SRC_DIR --image named\$")" -eq 1 ]; then
    log_success "configure <leaf> --image named, no leaf-system call"
else
    fail "case C: calls:
$LOG"
fi

log_header "D. PROJECT named: the project is configured, not DIR"
mkdir -p "$TEST_TMPDIR/ws"
run_case d 1 -DPROBE_IMAGE=bringup:img "-DPROBE_PROJECT=$TEST_TMPDIR/ws"
check
if [ "$RC" -eq 0 ] && [ "$(count "^configure $TEST_TMPDIR/ws --image bringup:img\$")" -eq 1 ]; then
    log_success "configure <ws> --image bringup:img"
else
    fail "case D: calls:
$LOG"
fi

log_header "I. ALL_IMAGES: every image configured, none located"
run_case i 1 -DPROBE_ALL=ON
check
if [ "$RC" -eq 0 ] && [ "$(count "^configure $SRC_DIR\$")" -eq 1 ] \
    && [ "$(count '^locate')" -eq 0 ] && [ "$(count '^dev image')" -eq 0 ]; then
    log_success "configure <project> (no --image), no locate"
else
    fail "case I: calls:
$LOG"
fi

log_header "J. IMAGES (two): each configured, none located"
run_case j 1 "-DPROBE_IMAGES=a;b"
check
if [ "$RC" -eq 0 ] && [ "$(count "^configure $SRC_DIR --image a\$")" -eq 1 ] \
    && [ "$(count "^configure $SRC_DIR --image b\$")" -eq 1 ] \
    && [ "$(count '^locate')" -eq 0 ]; then
    log_success "configure --image a, --image b; no locate"
else
    fail "case J: calls:
$LOG"
fi

log_header "K. IMAGES (one): configured AND located"
run_case k 1 "-DPROBE_IMAGES=only"
check
if [ "$RC" -eq 0 ] && [ "$(count "^configure $SRC_DIR --image only\$")" -eq 1 ] \
    && [ "$(count "^locate configured --project $SRC_DIR --image only")" -eq 1 ]; then
    log_success "configure --image only; located"
else
    fail "case K: calls:
$LOG"
fi

log_header "E. a guest (no system.toml): unchanged, no CLI call"
run_case e 0
check
if [ "$RC" -eq 0 ] && [ -z "$LOG" ] && nros_grep_q 'PROBE_CURRENT=$' <<<"$OUT"; then
    log_success "no call, not current"
else
    fail "case E: rc=$RC calls:
$LOG"
fi

log_header "F. inside the census's host build: no CLI call"
export NROS_LEAF_CENSUS_HOST=1
run_case f 1
unset NROS_LEAF_CENSUS_HOST
check
if [ "$RC" -eq 0 ] && [ -z "$LOG" ]; then
    log_success "NROS_LEAF_CENSUS_HOST: no configure"
else
    fail "case F: calls:
$LOG"
fi

log_header "L. a stale stamp after configure: located, NOT current (Ruling R33)"
export STANDIN_STALE=1
run_case l 1 -DPROBE_IMAGE=x
unset STANDIN_STALE
check
if [ "$RC" -eq 0 ] && [ "$(count '^locate configured')" -eq 1 ] \
    && nros_grep_q 'PROBE_CURRENT=FALSE' <<<"$OUT"; then
    log_success "a stale answer leaves the configure unseeded"
else
    fail "case L: rc=$RC calls:
$LOG
$OUT"
fi

log_header "G. configure fails: the host configure fails, quoting it"
export STANDIN_FAIL=1
run_case g 1 -DPROBE_IMAGE=x
unset STANDIN_FAIL
check
if [ "$RC" -ne 0 ] && nros_grep_q 'configure: boom' <<<"$OUT"; then
    log_success "FATAL, quoting configure's error"
else
    fail "case G: rc=$RC
$OUT"
fi

# ---------------------------------------------------------------------------
log_header "M. a re-configure of the same build dir configures again"
# The once-per-configure guard is a GLOBAL property: it must not survive into
# the next configure of the same build dir (a CACHE spelling would, and the
# second configure would read a stale answer with no `nros configure` first).
run_case m 1 -DPROBE_IMAGE=x
: > "$CALLS"
RC=0
OUT="$(STANDIN_LOG="$CALLS" NROS_CLI="$STANDIN" cmake "$SRC_DIR/build" 2>&1)" || RC=$?
LOG="$(cat "$CALLS")"
check
if [ "$RC" -eq 0 ] && [ "$(count "^configure $SRC_DIR --image x\$")" -eq 1 ]; then
    log_success "the second configure ran \`nros configure\` again"
else
    fail "case M: the guard survived into the re-configure (rc=$RC):
$LOG"
fi

# ---------------------------------------------------------------------------
log_header "N. one image-selection order: CONFIGURED_IMAGE, then NROS_IMAGE"
# `nros_configure_hook_image` is the order the root CMakeLists.txt and
# `nano_ros_workspace()` share.
N_SRC="$TEST_TMPDIR/n"
mkdir -p "$N_SRC"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.20)' \
    'project(n NONE)' \
    "include(\"$HOOK_MODULE\")" \
    'nros_configure_hook_image(_img)' \
    'message(STATUS "PROBE_IMG=${_img}")' > "$N_SRC/CMakeLists.txt"
img_of() { cmake -S "$N_SRC" -B "$N_SRC/b-$1" "${@:2}" 2>&1 | sed -n 's/.*PROBE_IMG=//p'; }
check
a="$(img_of 1 -DNROS_CONFIGURED_IMAGE=b:c -DNROS_IMAGE=i)"
b="$(img_of 2 -DNROS_IMAGE=i)"
c="$(img_of 3)"
if [ "$a" = "b:c" ] && [ "$b" = "i" ] && [ -z "$c" ]; then
    log_success "CONFIGURED_IMAGE > NROS_IMAGE > (the caller's default)"
else
    fail "case N: got '$a' / '$b' / '$c' (want b:c / i / empty)"
fi

# ---------------------------------------------------------------------------
log_header "O. R31: only a generated application walks up to its workspace"
# The Zephyr hook's caller decides the project (RFC-0085 D14): a generated
# application (NROS_IMAGE_SYSTEM_DIR stated) configures the workspace that owns
# its bringup; a standalone leaf that happens to sit under `src/` is itself.
O_WS="$TEST_TMPDIR/o-ws"
mkdir -p "$O_WS/src/leaf" "$O_WS/src/bringup"
touch "$O_WS/src/leaf/system.toml" "$O_WS/src/bringup/system.toml"
o_project() { # <dir> <generated>
    : > "$CALLS"
    printf '%s\n' \
        "include(\"$PROJECT_ROOT/zephyr/cmake/nros_image_kconfig.cmake\")" \
        "_nros_image_kconfig_configure(\"$STANDIN\" \"\${DIR}\" img \"\${GEN}\")" \
        > "$TEST_TMPDIR/o.cmake"
    STANDIN_LOG="$CALLS" cmake -DDIR="$1" -DGEN="$2" -P "$TEST_TMPDIR/o.cmake" >/dev/null 2>&1
    sed -n 's/^configure \([^ ]*\) --image img$/\1/p' "$CALLS" | head -1
}
check
got_leaf="$(o_project "$O_WS/src/leaf" FALSE)"
got_gen="$(o_project "$O_WS/src/bringup" TRUE)"
if [ "$got_leaf" = "$O_WS/src/leaf" ] && [ "$got_gen" = "$O_WS" ]; then
    log_success "standalone leaf -> itself; generated app -> its workspace"
else
    fail "case O: leaf '$got_leaf' (want $O_WS/src/leaf), generated '$got_gen' (want $O_WS)"
fi

# ---------------------------------------------------------------------------
log_header "P. R38: an edit to a digested input re-runs the host configure"
# `nros locate configured` names the files the stamp's digest covered; the
# hook makes each a configure dependency, so editing a launch file (not
# system.toml / package.xml) re-runs the configure, and its hook re-runs
# `nros configure`.
if ! command -v ninja >/dev/null 2>&1; then
    fail "ninja is not on PATH -- case P cannot report a verdict without it"
else
    P_SRC="$TEST_TMPDIR/p"
    write_project "$P_SRC"
    printf '[system]\nname = "probe"\n' > "$P_SRC/system.toml"
    mkdir -p "$P_SRC/launch"
    printf '<launch/>\n' > "$P_SRC/launch/talk.launch.xml"
    : > "$CALLS"
    P_RC=0
    STANDIN_LOG="$CALLS" NROS_CLI="$STANDIN" STANDIN_INPUTS="$P_SRC/launch/talk.launch.xml" \
        cmake -G Ninja -S "$P_SRC" -B "$P_SRC/build" -DPROBE_IMAGE=x >/dev/null 2>&1 || P_RC=$?
    sleep 1
    printf '<launch><node/></launch>\n' > "$P_SRC/launch/talk.launch.xml"
    : > "$CALLS"
    P_OUT="$(STANDIN_LOG="$CALLS" NROS_CLI="$STANDIN" \
        STANDIN_INPUTS="$P_SRC/launch/talk.launch.xml" ninja -C "$P_SRC/build" 2>&1)" || P_RC=$?
    LOG="$(cat "$CALLS")"
    SRC_DIR="$P_SRC"
    check
    if [ "$P_RC" -eq 0 ] && nros_grep_q 'Re-running CMake' <<<"$P_OUT" \
        && [ "$(count "^configure $P_SRC --image x\$")" -eq 1 ]; then
        log_success "launch-file edit -> host re-configure -> nros configure"
    else
        fail "case P: rc=$P_RC, no re-configure on a digested input's edit:
$P_OUT
$LOG"
    fi
fi

# ---------------------------------------------------------------------------
log_header "H. reach: every hooked entry calls the helper; one spelling"
for f in nano_rosConfig.cmake CMakeLists.txt cmake/NanoRosWorkspace.cmake \
         zephyr/cmake/nros_image_kconfig.cmake; do
    check
    if nros_grep_q 'nros_configure_hook(' <"$PROJECT_ROOT/$f"; then
        log_success "$f calls nros_configure_hook"
    else
        fail "$f does not call nros_configure_hook (RFC-0120 D5: it is a hooked entry)"
    fi
done
check
# Any `COMMAND <cli> configure` outside the helper is a second spelling.
nros_git_grep others -C "$PROJECT_ROOT" -nE 'COMMAND +"?\$\{[A-Za-z_]+\}"? +configure( |$)' \
    -- 'cmake/*.cmake' 'zephyr/*.cmake' '*CMakeLists.txt' 'nano_rosConfig.cmake'
others="$(grep -v '^cmake/NanoRosConfigureHook.cmake:' <<<"$others" || true)"
if [ -z "$others" ]; then
    log_success "only NanoRosConfigureHook.cmake spells the configure command"
else
    fail "a second spelling of the configure command:
$others"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
    log_success "cmake-configure-hook: $CHECKS checks held"
    exit 0
fi
log_error "cmake-configure-hook: $FAILURES of $CHECKS checks failed"
exit 1
