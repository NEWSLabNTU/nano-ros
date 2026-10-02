#!/bin/bash
# tests/cmake-bounds-fragment-configure-tests.sh -- issue 1647
#
# The canonical (build-time) interface generator must leave the MESSAGE-BOUND
# FRAGMENT on disk by the end of the CONFIGURE, because the configure is where
# it is read: `nros_find_interfaces()` composes it into
# `nros/message_bound_knobs.cmake`, `nano_ros_entry()` prices the service and
# action inboxes from it, and the answers are baked into the cargo commands of
# `nros-c` / `nros-cpp` / the RMW backend.
#
# It used to be a build-time custom-command output only. Measured on
# `examples/workspaces/cpp` `demo_bringup:native`: from a clean build dir every
# configure pass of the FIRST `nros build` composed "5 of 5 message-bound
# fragments have not been written yet", the image compiled every derived
# message-bound knob at its placeholder, and the SECOND `nros build` produced a
# different `native_entry` (the fixed point). A fixture build builds once, so
# every fixture of a cmake workspace image was the unsettled one.
#
# WHAT IS ASSERTED
#
#   A. A CLEAN configure, with no build at all, leaves the fragment on disk and
#      it is a real fragment (it states the schema and prices the package's
#      type). Before the fix the file did not exist until the build ran.
#
#   B. An interface file is a CONFIGURE dependency of the directory that
#      generates it. Without that edge a `.msg` edit is picked up by the
#      build-time command mid-build, AFTER the configure that read the old
#      fragment -- the same lag, one build later.
#
#   C. A RE-configure after an edit that moves the bound re-emits the fragment
#      with the NEW bound. That is the behaviour the edge in B buys, measured
#      rather than inferred from the edge's presence.
#
#   D. Negative control for C: a re-configure with NOTHING changed leaves the
#      fragment's bytes AND mtime alone -- so the pre-emit is not a codegen run
#      on every configure, which would be the cost of getting the predicate
#      wrong in the other direction.
#
# PRECONDITIONS ARE HARD FAILURES. The `just` recipe owns skipping (through the
# check ledger); a run of this script that cannot reach a verdict fails.
#
# Usage: ./tests/cmake-bounds-fragment-configure-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

FAILURES=0
CHECKS=0

fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

check() {
    CHECKS=$((CHECKS + 1))
}

if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

NROS_BIN="$PROJECT_ROOT/packages/cli/target/release/nros"
if [ ! -x "$NROS_BIN" ]; then
    fail "in-tree CLI not built at $NROS_BIN -- run \`just setup-cli\` (codegen has no other producer)"
    exit 1
fi
# The cmake modules resolve the CLI from the environment before anything else;
# a DIFFERENT checkout's binary would generate with that checkout's codegen.
export NROS_CLI="$NROS_BIN"
export PATH="$PROJECT_ROOT/packages/cli/target/release:$PATH"
export NROS_REPO_DIR="$PROJECT_ROOT"
unset NANO_ROS_GEN_CACHE_DIR

init_test_tmpdir "nros-bounds-fragment-configure"
trap 'cleanup_test_tmpdir' EXIT

SRC="$TEST_TMPDIR/probe_msgs"
BUILD="$TEST_TMPDIR/build"
mkdir -p "$SRC/msg" "$BUILD"

# A workspace-local interface package, so the test owns the `.msg` it edits and
# needs no ament install. C, so no cargo / Corrosion is reached at configure.
printf '%s\n' 'int32 value' > "$SRC/msg/Probe.msg"
{
    echo 'cmake_minimum_required(VERSION 3.22)'
    echo 'project(probe_msgs LANGUAGES C)'
    echo "set(_NANO_ROS_PREFIX \"$PROJECT_ROOT\")"
    echo "set(_NANO_ROS_CODEGEN_TOOL \"$NROS_BIN\" CACHE FILEPATH \"\")"
    echo "include(\"$PROJECT_ROOT/cmake/NanoRosGenerateInterfaces.cmake\")"
    echo 'nros_generate_interfaces(probe_msgs LANGUAGE C SKIP_INSTALL)'
    echo 'message(STATUS "NROS_TEST_FRAGMENT=${probe_msgs_MESSAGE_BOUNDS_CMAKE}")'
} > "$SRC/CMakeLists.txt"

configure() {
    cmake -S "$SRC" -B "$BUILD" -G "Unix Makefiles" > "$TEST_TMPDIR/configure.log" 2>&1
}

# The TX bound the fragment states for probe_msgs/msg/Probe, or empty.
fragment_tx() {
    sed -n 's/^set(NROS_MESSAGE_BOUND_probe_msgs_msg_Probe_TX \([0-9]*\)).*/\1/p' "$FRAGMENT"
}

# ---------------------------------------------------------------------------
log_info "A. a clean configure leaves the bound fragment on disk"
check
if ! configure; then
    fail "the probe project did not configure:"
    cat "$TEST_TMPDIR/configure.log" >&2
    exit 1
fi
FRAGMENT="$(sed -n 's/.*NROS_TEST_FRAGMENT=//p' "$TEST_TMPDIR/configure.log" | tail -1)"
if [ -z "$FRAGMENT" ]; then
    fail "the probe did not report where the generator places the fragment"
    exit 1
fi
if [ ! -f "$FRAGMENT" ]; then
    fail "after a CONFIGURE, $FRAGMENT does not exist -- the fragment is still a build-time-only output, so every configure pass of a clean build dir's first build composes 'not written yet' (issue 1647)"
else
    check
    if ! nros_grep_q 'NROS_MESSAGE_BOUNDS_SCHEMA_VERSION' "$FRAGMENT"; then
        fail "$FRAGMENT exists but states no schema version"
    fi
    TX_BEFORE="$(fragment_tx)"
    check
    if [ "$TX_BEFORE" != "8" ]; then
        fail "the fragment prices probe_msgs/msg/Probe (one int32) at TX='$TX_BEFORE', expected 8 (4-byte CDR encapsulation + one int32)"
    fi
fi

# ---------------------------------------------------------------------------
log_info "B. the interface file is a configure dependency of the generating directory"
check
if ! nros_grep_q -F -- "$SRC/msg/Probe.msg" "$BUILD/CMakeFiles/Makefile.cmake"; then
    fail "$SRC/msg/Probe.msg is not in CMAKE_MAKEFILE_DEPENDS -- an edit that moves a bound is not seen until the build-time command rewrites the fragment, after the configure that read it"
fi

# ---------------------------------------------------------------------------
log_info "D. a re-configure with nothing changed leaves the fragment alone"
if [ -f "$FRAGMENT" ]; then
    MTIME_BEFORE="$(stat -c '%Y.%y' "$FRAGMENT")"
    SUM_BEFORE="$(sha256sum "$FRAGMENT" | cut -d' ' -f1)"
    sleep 1
    configure || { fail "re-configure failed"; cat "$TEST_TMPDIR/configure.log" >&2; }
    check
    if [ "$(stat -c '%Y.%y' "$FRAGMENT")" != "$MTIME_BEFORE" ] \
        || [ "$(sha256sum "$FRAGMENT" | cut -d' ' -f1)" != "$SUM_BEFORE" ]; then
        fail "an unchanged re-configure rewrote $FRAGMENT -- the pre-emit predicate fires when nothing moved"
    fi
fi

# ---------------------------------------------------------------------------
log_info "C. a re-configure after a bound-moving edit re-emits the fragment"
if [ -f "$FRAGMENT" ]; then
    sleep 1
    printf '%s\n' 'int32 value' 'int64 wide' > "$SRC/msg/Probe.msg"
    configure || { fail "re-configure failed"; cat "$TEST_TMPDIR/configure.log" >&2; }
    TX_AFTER="$(fragment_tx)"
    check
    if [ -z "$TX_AFTER" ] || [ "$TX_AFTER" = "$TX_BEFORE" ]; then
        fail "after adding an int64 to Probe.msg and re-configuring, the fragment still says TX='$TX_AFTER' (was '$TX_BEFORE') -- the configure read a stale bound"
    fi
fi

echo
if [ "$FAILURES" -gt 0 ]; then
    log_error "$FAILURES of $CHECKS checks failed"
    exit 1
fi
log_success "all $CHECKS checks passed"
