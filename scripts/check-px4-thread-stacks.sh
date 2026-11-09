#!/usr/bin/env bash
# check-px4-thread-stacks.sh -- issue 2398.
#
# A generated PX4 entry runs nano-ros on its module's main task (which opens the
# session) and on one PX4 work queue per sched task (which runs every pass).
# PX4's stock sizes are below the executor's stack floor on NuttX: `wq:rate_ctrl`
# is 3150 B and a builtin's STACK_MAIN defaults to 2048 B, against a floor of
# 3808 B for fmu-v6x. `NanoRosPx4ThreadStacks.cmake` refuses that at configure
# time, naming the knob. This gate shows the refusal FIRES, in both directions,
# with the stock numbers, and that the module calls it.
#
# Three claims:
#   1. THE PREDICATE FIRES -- the real cmake functions, run under `cmake -P`
#      against synthetic board values: stock queue and stock main task refused,
#      raised values accepted, POSIX (SITL) accepted with stock values, and the
#      floor read from a config header shaped like the generated one.
#   2. THE QUEUES ARE THE ENTRY'S -- the sched-task -> queue pairs are read from
#      the codegen's own PX4 golden output, so a change to the constructor shape
#      the entry emits breaks this rather than silently matching nothing.
#   3. WIRING -- `NanoRosPx4Module.cmake` includes the file and calls all three
#      functions.
#
# Needs no PX4 tree and no build: `cmake -P` plus files this script writes.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODULE="$ROOT/integrations/px4/NanoRosPx4ThreadStacks.cmake"
CONSUMER="$ROOT/integrations/px4/NanoRosPx4Module.cmake"
GOLDEN="$ROOT/packages/cli/nros-cli-core/testdata/entry/px4_sched_tasks.cpp.golden"

# shellcheck source=scripts/build/check-skip.sh
source "$ROOT/scripts/build/check-skip.sh"
command -v cmake >/dev/null 2>&1 || {
    nros_check_unverified px4-thread-stacks "cmake not on PATH" || exit 1
    exit 0
}

fail=0
bad() { printf 'FAIL: %s\n' "$*" >&2; fail=1; }
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# A header shaped like the generated one (nros-build-helpers `cpp.rs`).
printf '%s\n' \
    '#define NROS_EXECUTOR_VALUE_SIZE 1904' \
    '#define NROS_EXECUTOR_MAIN_STACK_MIN 3808' \
    '#if CONFIG_MAIN_STACK_SIZE < NROS_EXECUTOR_MAIN_STACK_MIN' \
    '#endif' >"$TMP/header.h"
printf '%s\n' '#define NROS_EXECUTOR_VALUE_SIZE 1904' >"$TMP/no-floor.h"
printf '%s\n' '#define NROS_EXECUTOR_MAIN_STACK_MIN 30000' >"$TMP/big-floor.h"

printf '%s\n' \
    'include("${M}")' \
    'nros_px4_read_stack_floor(floor "${HEADER}")' \
    'if(NOT floor EQUAL EXPECT_FLOOR)' \
    '    message(FATAL_ERROR "floor read as ${floor}, expected ${EXPECT_FLOOR}")' \
    'endif()' \
    'string(REPLACE "," ";" queues "${QUEUES}")' \
    'nros_px4_check_thread_stacks(MODULE m PLATFORM "${PLATFORM}" POINTER_SIZE "${PTR}"' \
    '    FLOOR "${floor}" FLOOR_SOURCE "${HEADER}" QUEUES ${queues} STACK_MAIN "${MAIN}")' \
    'message(STATUS "accepted")' >"$TMP/check.cmake"

printf '%s\n' \
    'include("${M}")' \
    'nros_px4_entry_work_queues(q "${ENTRY}")' \
    'string(REPLACE ";" "," q "${q}")' \
    'message(STATUS "QUEUES=${q}")' >"$TMP/queues.cmake"

# case <name> <expect accept|refuse> <must-mention or -> <cmake -D args...>
case_() {
    local name="$1" expect="$2" mention="$3"
    shift 3
    local rc=0
    cmake -DM="$MODULE" -DHEADER="$TMP/header.h" -DEXPECT_FLOOR=3808 -DPTR=4 \
        -DMAIN= "$@" -P "$TMP/check.cmake" >"$TMP/out.txt" 2>&1 || rc=$?
    if [ "$expect" = accept ] && [ $rc -ne 0 ]; then
        bad "$name: refused, expected accepted:"; sed 's/^/    /' "$TMP/out.txt" >&2
    elif [ "$expect" = refuse ] && [ $rc -eq 0 ]; then
        bad "$name: ACCEPTED, expected a refusal"
    elif [ "$mention" != - ] && [[ "$(cat "$TMP/out.txt")" != *"$mention"* ]]; then
        bad "$name: output does not name '$mention':"; sed 's/^/    /' "$TMP/out.txt" >&2
    else
        printf '  ok  %s\n' "$name"
    fi
}

echo "check-px4-thread-stacks: the stack-floor refusal must fire (issue 2398)"

# The negative controls ARE the check: every case runs on the normal path,
# so a refusal that stops firing fails this script.
self_test() {
    local rc out

    # --- NuttX, the board PX4 ships: refused on both threads ------------------
    case_ "stock wq:rate_ctrl (3150) on NuttX" refuse CONFIG_WQ_RATE_CTRL_STACKSIZE \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=3150 -DMAIN=8192
    case_ "stock STACK_MAIN (unset, 2048) on NuttX" refuse "passes no STACK_MAIN" \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=8192
    case_ "STACK_MAIN 2048 on NuttX" refuse "STACK_MAIN is 2048" \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=8192 -DMAIN=2048
    case_ "second queue short (hp_default 2800)" refuse CONFIG_WQ_HP_DEFAULT_STACKSIZE \
        -DPLATFORM=nuttx "-DQUEUES=high=rate_ctrl,low=hp_default" \
        -DCONFIG_WQ_RATE_CTRL_STACKSIZE=8192 -DCONFIG_WQ_HP_DEFAULT_STACKSIZE=2800 -DMAIN=8192
    case_ "queue option missing from the board config" refuse "sets no CONFIG_WQ_RATE_CTRL_STACKSIZE" \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DMAIN=8192
    case_ "STACK_MAIN not a byte count" refuse "not a number of bytes" \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=8192 -DMAIN=4k
    case_ "one below the floor" refuse CONFIG_WQ_RATE_CTRL_STACKSIZE \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=3807 -DMAIN=8192

    # --- accepted ---------------------------------------------------------------
    case_ "raised queue and STACK_MAIN on NuttX" accept - \
        -DPLATFORM=nuttx "-DQUEUES=high=rate_ctrl,low=hp_default" \
        -DCONFIG_WQ_RATE_CTRL_STACKSIZE=8192 -DCONFIG_WQ_HP_DEFAULT_STACKSIZE=8192 -DMAIN=16384
    case_ "exactly the floor" accept - \
        -DPLATFORM=nuttx -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=3808 -DMAIN=3808
    # SITL: PX4 doubles a 64-bit stack and adds 24 KiB, and STACK_MAIN sizes nothing.
    case_ "stock SITL (posix, 64-bit)" accept - \
        -DPLATFORM=posix -DPTR=8 "-DQUEUES=high=rate_ctrl,low=hp_default" \
        -DCONFIG_WQ_RATE_CTRL_STACKSIZE=3150 -DCONFIG_WQ_HP_DEFAULT_STACKSIZE=2800
    # 1000 is Kconfig's minimum: doubled it is below the floor, so only PX4's POSIX
    # overhead lets it pass. A check that forgot the overhead refuses every SITL.
    case_ "SITL at Kconfig's minimum (1000)" accept - \
        -DPLATFORM=posix -DPTR=8 -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=1000
    # A 64-bit build doubles the configured size before adding the overhead:
    # 3150 * 2 + 24576 = 30876 holds a 30000 floor, 3150 + 24576 would not.
    case_ "SITL doubles a 64-bit stack" accept - -DHEADER="$TMP/big-floor.h" -DEXPECT_FLOOR=30000 \
        -DPLATFORM=posix -DPTR=8 -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=3150
    # ... and the 64-bit doubling is PX4's too: a 32-bit qurt build gets no overhead.
    case_ "qurt at 3150 (no overhead, no doubling)" refuse CONFIG_WQ_RATE_CTRL_STACKSIZE \
        -DPLATFORM=qurt -DQUEUES=high=rate_ctrl -DCONFIG_WQ_RATE_CTRL_STACKSIZE=3150

    # --- the floor comes from the header ----------------------------------------
    rc=0
    cmake -DM="$MODULE" -DHEADER="$TMP/no-floor.h" -DEXPECT_FLOOR=0 -DPTR=4 -DMAIN=8192 \
        -DPLATFORM=nuttx -DQUEUES= -P "$TMP/check.cmake" >"$TMP/out.txt" 2>&1 || rc=$?
    if [ $rc -eq 0 ] || [[ "$(cat "$TMP/out.txt")" != *"defines no"* ]]; then
        bad "a header with no NROS_EXECUTOR_MAIN_STACK_MIN was not refused"
    else
        printf '  ok  %s\n' "header without the floor refused"
    fi

    # --- the queues are the generated entry's -----------------------------------
    if [ ! -f "$GOLDEN" ]; then
        bad "codegen golden missing: $GOLDEN"
    else
        out="$(cmake -DM="$MODULE" -DENTRY="$GOLDEN" -P "$TMP/queues.cmake" 2>&1)"
        if [[ "$out" == *"QUEUES=nros_demo_high=rate_ctrl,nros_demo_low=hp_default"* ]]; then
            printf '  ok  %s\n' "sched tasks -> queues read from the codegen golden"
        else
            bad "queues read from $GOLDEN: '$out'"
        fi
    fi
}
self_test

# --- wiring -----------------------------------------------------------------
consumer="$(cat "$CONSUMER")"
for want in "NanoRosPx4ThreadStacks.cmake" "nros_px4_read_stack_floor(" \
    "nros_px4_entry_work_queues(" "nros_px4_check_thread_stacks("; do
    if [[ "$consumer" == *"$want"* ]]; then
        printf '  ok  %s\n' "NanoRosPx4Module.cmake names $want"
    else
        bad "NanoRosPx4Module.cmake does not name $want"
    fi
done

if [ $fail -ne 0 ]; then
    echo "check-px4-thread-stacks: FAILED" >&2
    exit 1
fi
echo "check-px4-thread-stacks: OK"
