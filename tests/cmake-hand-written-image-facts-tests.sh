#!/usr/bin/env bash
# tests/cmake-hand-written-image-facts-tests.sh -- GitHub #3856 / #3866
# (issues 2564, 2582).
#
# WHAT THIS MEASURES
#
# A hand-written CMake configure that NAMES its image must get that image's
# facts, through the one reader (`nros dev image leaf-system`), as a standalone
# leaf and a generated root already do:
#
#   A  nros_image_domain_id      asks the reader for THAT image (`--image`) of
#                                the bringup and returns its stated domain
#   B  ... no system.toml        returns "" and asks nothing
#   C  read_leaf_system CLAIM    a bringup that is not the top-level project,
#                                read for a named image, speaks for the
#                                configure: the image env reaches the carrier
#                                (NROS_IMAGE_ENV_CONFIG) and the domain is the
#                                configure's (NROS_TOP_LEAF_DOMAIN_ID)
#   D  ... without CLAIM         a member's read claims neither, and warns
#   E  CLAIM + disagreeing -D    refused, naming both domains
#   F  REACH                     nano_ros_workspace() claims its named image
#                                and refuses unread per-image rows;
#                                nano_ros_add_executable() asks the image's
#                                domain when it names BRINGUP and IMAGE
#
# The CLI is a STAND-IN that records each call ($NROS_CLI is the resolver's
# first rung). PRECONDITIONS ARE HARD FAILURES (issue 1758): no cmake, red.
#
# Usage: ./tests/cmake-hand-written-image-facts-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0
CHECKS=0
fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}
check() { CHECKS=$((CHECKS + 1)); }

if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

init_test_tmpdir "nros-hand-written-image-facts"
trap 'cleanup_test_tmpdir' EXIT

# ---------------------------------------------------------------------------
# The stand-in CLI: logs argv and answers `dev image leaf-system` for the image
# named with `--image` (domain 2, one knob row), writing the image env file
# when `--image-env-out` asks for it.
# ---------------------------------------------------------------------------
STANDIN="$TEST_TMPDIR/nros"
CALLS="$TEST_TMPDIR/calls.log"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "%s\n" "$*" >> "$STANDIN_LOG"' \
    'image=""; env_out=""' \
    'while [ $# -gt 0 ]; do' \
    '  case "$1" in' \
    '    --image) image="$2"; shift ;;' \
    '    --image-env-out) env_out="$2"; shift ;;' \
    '  esac; shift' \
    'done' \
    'echo "NROS_LEAF_DEPLOY=posix"' \
    'echo "NROS_LEAF_BOARD=linux"' \
    'echo "NROS_LEAF_IMAGE=${image:-default_img}"' \
    'echo "NROS_LEAF_DOMAIN_ID=2"' \
    'if [ -n "$env_out" ]; then' \
    '  mkdir -p "$(dirname "$env_out")"' \
    '  printf "[env]\nNROS_MAX_PARAMETERS = \"164\"\n" > "$env_out"' \
    '  echo "NROS_LEAF_IMAGE_ENV=$env_out"' \
    'fi' > "$STANDIN"
chmod +x "$STANDIN"

# $1 = case dir, $2 = the probe body (cmake), run after the modules load.
write_project() {
    local dir="$1" body="$2"
    mkdir -p "$dir/src/bringup"
    printf '[system]\nname = "bringup"\n' > "$dir/src/bringup/system.toml"
    printf '%s\n' \
        'cmake_minimum_required(VERSION 3.20)' \
        'project(nros_image_facts_probe NONE)' \
        "set(NROS_REPO_DIR \"$PROJECT_ROOT\")" \
        "include(\"$PROJECT_ROOT/cmake/NanoRosCli.cmake\")" \
        "include(\"$PROJECT_ROOT/cmake/NanoRosPackageXml.cmake\")" \
        "$body" > "$dir/CMakeLists.txt"
}

# run_case <name> <body> [cmake -D args...]; sets OUT, RC, LOG, SRC_DIR.
run_case() {
    local name="$1" body="$2"
    shift 2
    local src="$TEST_TMPDIR/$name"
    write_project "$src" "$body"
    : > "$CALLS"
    RC=0
    OUT="$(STANDIN_LOG="$CALLS" NROS_CLI="$STANDIN" \
        cmake -S "$src" -B "$src/build" "$@" 2>&1)" || RC=$?
    LOG="$(cat "$CALLS")"
    SRC_DIR="$src"
    # cmake wraps a message across lines; FLAT is OUT on one line.
    FLAT="$(tr -s '\n ' '  ' <<<"$OUT")"
}

count() { grep -c -- "$1" <<<"$LOG" || true; }

# ---------------------------------------------------------------------------
log_header "A. nros_image_domain_id: that image's stated domain"
run_case a '
nros_image_domain_id(_d DIR "${CMAKE_SOURCE_DIR}/src/bringup" IMAGE an536)
message(STATUS "PROBE_DOMAIN=${_d}")'
check
if [ "$RC" -eq 0 ] \
    && [ "$(count "^dev image leaf-system $SRC_DIR/src/bringup .*--image an536\$")" -eq 1 ] \
    && nros_grep_q 'PROBE_DOMAIN=2' <<<"$OUT"; then
    log_success "asked for --image an536 of the bringup; domain 2"
else
    fail "case A: rc=$RC calls:
$LOG
$OUT"
fi

log_header "B. nros_image_domain_id with no system.toml: nothing asked"
run_case b '
nros_image_domain_id(_d DIR "${CMAKE_SOURCE_DIR}/src/none" IMAGE an536)
message(STATUS "PROBE_DOMAIN=[${_d}]")'
check
if [ "$RC" -eq 0 ] && [ -z "$LOG" ] && nros_grep_q 'PROBE_DOMAIN=\[\]' <<<"$OUT"; then
    log_success "no CLI call, empty answer"
else
    fail "case B: rc=$RC calls:
$LOG
$OUT"
fi

PROBE_READ='
nano_ros_read_leaf_system(DIR "${CMAKE_SOURCE_DIR}/src/bringup" ${PROBE_ARGS})
get_property(_env GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG)
get_property(_dom GLOBAL PROPERTY NROS_TOP_LEAF_DOMAIN_ID)
message(STATUS "PROBE_ENV=[${_env}]")
message(STATUS "PROBE_TOP_DOMAIN=[${_dom}]")'

log_header "C. a CLAIMed bringup read speaks for the configure"
run_case c "$PROBE_READ" "-DPROBE_ARGS=IMAGE;freertos-posix;CLAIM"
check
if [ "$RC" -eq 0 ] \
    && [ "$(count -- '--image freertos-posix --image-env-out ')" -eq 1 ] \
    && nros_grep_q "PROBE_ENV=\[$SRC_DIR/build/nros/nros-image-env.toml\]" <<<"$OUT" \
    && nros_grep_q 'PROBE_TOP_DOMAIN=\[2\]' <<<"$OUT"; then
    log_success "image env attached and the image's domain is the configure's"
else
    fail "case C: rc=$RC calls:
$LOG
$OUT"
fi

log_header "D. the same read without CLAIM stays a member's"
run_case d "$PROBE_READ" "-DPROBE_ARGS=IMAGE;freertos-posix"
check
if [ "$RC" -eq 0 ] \
    && nros_grep_q 'PROBE_ENV=\[\]' <<<"$OUT" \
    && nros_grep_q 'PROBE_TOP_DOMAIN=\[\]' <<<"$OUT" \
    && nros_grep_q 'is not the top-level project' <<<"$FLAT"; then
    log_success "no env claimed, no domain published, and it says why"
else
    fail "case D: rc=$RC calls:
$LOG
$OUT"
fi

log_header "E. a CLAIMed read refuses a disagreeing -DNROS_DOMAIN_ID"
run_case e "$PROBE_READ" "-DPROBE_ARGS=IMAGE;freertos-posix;CLAIM" -DNROS_DOMAIN_ID=5
check
if [ "$RC" -ne 0 ] && nros_grep_q 'states domain 2' <<<"$FLAT" \
    && nros_grep_q 'NROS_DOMAIN_ID=5' <<<"$FLAT"; then
    log_success "refused, naming 2 and 5"
else
    fail "case E: rc=$RC
$OUT"
fi

# ---------------------------------------------------------------------------
log_header "F. REACH: both hand-written roads ask"
check
WS="$PROJECT_ROOT/cmake/NanoRosWorkspace.cmake"
ENTRY="$PROJECT_ROOT/cmake/NanoRosEntry.cmake"
if nros_grep_q 'IMAGE "${_nrw_image}" CLAIM)' "$WS" \
    && nros_grep_q 'names no image, so they would' "$WS" \
    && nros_grep_q 'nros_image_domain_id(_nra_domain DIR' "$ENTRY"; then
    log_success "nano_ros_workspace() claims its image; nano_ros_add_executable() asks its domain"
else
    fail "case F: a hand-written road no longer asks the reader"
fi

echo
if [ "$FAILURES" -eq 0 ]; then
    log_success "all $CHECKS checks passed"
    exit 0
fi
log_error "$FAILURES of $CHECKS checks failed"
exit 1
