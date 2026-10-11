#!/bin/bash
# tests/cmake-workspace-system-required-tests.sh -- issue 1261
#
# A WORKSPACE WHOSE BRINGUP DECLARES A CAPABILITY MUST NAME IT AS `SYSTEM`.
#
# `nano_ros_workspace()` resolves the capability axes (`NANO_ROS_FEATURES`)
# only from its `SYSTEM` argument, while codegen reads the same declaration
# through `nros sync`. A root that omitted SYSTEM therefore built nros-cpp
# without `param-services` and generated code that calls
# `nros_cpp_register_parameter_services`. The disagreement surfaced at LINK,
# as an undefined symbol in a build that had configured cleanly.
#
# WHAT IS ASSERTED
#
#   A. THE REFUSAL. A bringup declaring `param_services`, a root with no
#      SYSTEM: configure FAILS, naming `SYSTEM demo_bringup` and issue 1261.
#   B. SYSTEM GIVEN. The same workspace with `SYSTEM demo_bringup`: the
#      refusal does not fire. (The configure may stop later for reasons this
#      test does not own -- an unsynced workspace has no model -- so B asserts
#      the ABSENCE of the refusal, not a green configure.)
#   C. NO BRINGUP. A workspace with no bringup at all: "workspaces without a
#      Bringup pkg are valid", so the refusal does not fire and the configure
#      succeeds.
#
# A is the case the gate exists for, and B and C keep the refusal from being a
# blanket "SYSTEM is required", which would break every bringup-less
# workspace `nros build` generates.
#
# PRECONDITIONS ARE HARD FAILURES; skipping belongs in the `just` recipe.
#
# Usage: ./tests/cmake-workspace-system-required-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

for _t in cmake c++; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "$_t is not on PATH -- this test cannot report a verdict without it"
        exit 1
    fi
done

NROS_BIN="$PROJECT_ROOT/packages/cli/target/release/nros"
if [ ! -x "$NROS_BIN" ]; then
    fail "in-tree CLI not built at $NROS_BIN -- run \`just setup-cli\`"
    exit 1
fi
# Same env discipline as the sibling probe tests (issues 0363, 1280).
export PATH="$PROJECT_ROOT/packages/cli/target/release:$PATH"
export nano_ros_ROOT="$PROJECT_ROOT"
export NROS_WORKSPACE="$PROJECT_ROOT"
export NROS_REPO_DIR="$PROJECT_ROOT"

init_test_tmpdir "nros-workspace-system-required"
trap 'cleanup_test_tmpdir' EXIT

# write_ws <dir> <with-bringup:0|1>
write_ws() {
    local ws="$1"
    mkdir -p "$ws/root"
    if [ "$2" = 1 ]; then
        mkdir -p "$ws/src/demo_bringup"
        printf '%s\n' '<?xml version="1.0"?>' '<package format="3">' \
            '  <name>demo_bringup</name>' '  <version>0.0.0</version>' \
            '  <description>t</description>' '  <maintainer email="t@t">t</maintainer>' \
            '  <license>Apache-2.0</license>' '</package>' \
            > "$ws/src/demo_bringup/package.xml"
        printf '%s\n' '[system]' 'name = "t1261"' 'rmw = "zenoh"' 'domain_id = 0' \
            'features = ["param_services"]' \
            > "$ws/src/demo_bringup/system.toml"
    fi
}

# write_root <ws> <system-line>
write_root() {
    printf '%s\n' 'cmake_minimum_required(VERSION 3.22)' \
        'project(t1261 LANGUAGES C CXX)' \
        'find_package(nano_ros REQUIRED COMPONENTS workspace)' \
        'nano_ros_workspace(' '    WORKSPACE_ROOT ".."' '    BACKEND zenoh' \
        '    PLATFORM "posix"' "$2" '    SUBDIRS' ')' > "$1/root/CMakeLists.txt"
}

# configure <ws> <log> -> sets RC
configure() {
    RC=0
    cmake -S "$1/root" -B "$1/root/b" -Dnano_ros_ROOT="$PROJECT_ROOT" >"$2" 2>&1 || RC=$?
}

# ---- A: capability declared, SYSTEM omitted -> refused -------------------
A="$TEST_TMPDIR/a"
write_ws "$A" 1
write_root "$A" ""
configure "$A" "$TEST_TMPDIR/a.log"
if [ "$RC" -eq 0 ]; then
    fail "A: configure succeeded with SYSTEM omitted over a bringup declaring param_services"
fi
if ! nros_grep_q 'SYSTEM demo_bringup' "$TEST_TMPDIR/a.log" \
    || ! nros_grep_q 'issue 1261' "$TEST_TMPDIR/a.log"; then
    fail "A: the refusal does not name \`SYSTEM demo_bringup\` and issue 1261:"
    tail -15 "$TEST_TMPDIR/a.log" >&2
fi

# ---- B: SYSTEM given -> no refusal ------------------------------------------
B="$TEST_TMPDIR/b"
write_ws "$B" 1
write_root "$B" "    SYSTEM demo_bringup"
configure "$B" "$TEST_TMPDIR/b.log"
if nros_grep_q 'issue 1261' "$TEST_TMPDIR/b.log"; then
    fail "B: the refusal fired although SYSTEM was given"
    tail -15 "$TEST_TMPDIR/b.log" >&2
fi

# ---- C: no bringup -> no refusal, configure succeeds -----------------------
C="$TEST_TMPDIR/c"
write_ws "$C" 0
write_root "$C" ""
configure "$C" "$TEST_TMPDIR/c.log"
if [ "$RC" -ne 0 ] || nros_grep_q 'issue 1261' "$TEST_TMPDIR/c.log"; then
    fail "C: a bringup-less workspace did not configure cleanly (rc=$RC)"
    tail -15 "$TEST_TMPDIR/c.log" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    log_error "cmake-workspace-system-required: $FAILURES failure(s)"
    exit 1
fi
log_success "cmake-workspace-system-required: A refused, B and C not (issue 1261)"
exit 0
