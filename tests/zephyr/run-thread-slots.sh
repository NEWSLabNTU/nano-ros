#!/usr/bin/env bash
# phase-460 W6 -- the Zephyr stack-slot pool, on the HOST.
#
# Compiles the REAL `zephyr/nros_platform_zephyr_shims.c` against the stub
# Zephyr tree in `tests/zephyr/host-shims/` and runs
# `tests/zephyr/thread_slot_release_test.c` against it. See that file's header
# for what is under test and why it is testable here at all; see the stub
# tree's `kernel.h` for the rule those headers follow.
#
# No west, no Zephyr SDK and no board: `cc` and pthreads are the whole
# prerequisite, which is what puts this on the `just ci l1` gate lane rather
# than in the native_sim suite beside `run-c.sh`.
#
# THREE RUNS, and all three are the gate:
#
#   1. n-plus-2 at each capacity in POOL_SIZES -- the wave's gate proper.
#   2. detach at each capacity -- the negative control: a teardown that
#      detaches does NOT get its slot back.
#   3. the MUTANT: the same shims with the release compiled out from under it,
#      where run 1 must FAIL. A gate nobody has seen fail is a gate nobody has
#      measured.
#
# The capacities are swept because N reaches the shims and the test from ONE
# `-D` on the compile line; a single value could agree with a hardcoded bound
# by accident, and N=1 is the pool's own edge (issue 1015's floor).
#
# Usage:  ./tests/zephyr/run-thread-slots.sh [--verbose]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$PROJECT_ROOT"

VERBOSE=false
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=true ;;
        *) echo "unknown argument: $arg" >&2; exit 2 ;;
    esac
done

SHIMS="zephyr/nros_platform_zephyr_shims.c"
TEST_SRC="tests/zephyr/thread_slot_release_test.c"
MUTANT_SRC="tests/zephyr/thread_slot_leak_mutant.c"
STUBS="tests/zephyr/host-shims/include"

# `test -f` on every input before compiling anything: a renamed or moved source
# must read as a FAILURE, not as a gate with nothing to check (issue 0196's
# class -- the same reason `entry-seam-c-linkage` checks its TUs exist).
for f in "$SHIMS" "$TEST_SRC" "$MUTANT_SRC" "$STUBS/zephyr/kernel.h"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f is MISSING -- this gate would pass on absence." >&2
        exit 1
    fi
done

OUT="tmp/zephyr-thread-slots"
mkdir -p "$OUT"

# The capacities to sweep. 1 is the pool floor, 4 is a small pool that fills
# fast, 8 is the shims' own default.
POOL_SIZES=(1 4 8)

# 256 KiB per slot: comfortably over glibc's PTHREAD_STACK_MIN, which matters
# because `nros_zephyr_task_create_prio` hands these arrays straight to
# `pthread_attr_setstack`. It is also what CONFIG_MAIN_STACK_SIZE feeds, since
# NROS_ZEPHYR_STACK_SIZE defaults to it.
STACK_BYTES=262144

# CONFIG_PTHREAD is the gate on the whole thread-pool block. Everything left
# unset here -- CONFIG_NET_SOCKETS, CONFIG_SMP, CONFIG_SCHED_CPU_MASK,
# CONFIG_SCHED_DEADLINE, CONFIG_TRACING_CTF, CONFIG_NROS_SNTP_EPOCH -- removes
# an arm of the shims the stub tree would otherwise have to model.
common_cflags() {
    local pool="$1"
    printf '%s\n' \
        -std=c11 -Wall -Wextra -D_GNU_SOURCE \
        -DCONFIG_PTHREAD \
        "-DCONFIG_MAIN_STACK_SIZE=${STACK_BYTES}" \
        "-DNROS_ZEPHYR_STACK_SIZE=${STACK_BYTES}" \
        "-DNROS_ZEPHYR_MAX_THREADS=${pool}" \
        "-I${STUBS}" \
        -Itests/zephyr
}

failures=0
pass() { echo "PASS  $*"; }
fail() { echo "FAIL  $*"; failures=$((failures + 1)); }

show() {
    if [ "$VERBOSE" = true ]; then
        sed 's/^/      /' "$1"
    else
        sed 's/^/      /' "$1" | grep -E 'FAIL|assertion' || true
    fi
}

# The shims get their OWN compile, not a shared command line, because the
# mutant below defines a macro that must reach the shims and nothing else: on
# one command line it renames the mutant's replacement too, and the link fails
# with a duplicate symbol instead of producing the leaking shim.
build() {
    # build <binary> <pool> [shims-only cflags...]
    local bin="$1" pool="$2"
    shift 2
    local cflags=()
    mapfile -t cflags < <(common_cflags "$pool")
    local obj="${bin}.shims.o"
    cc "${cflags[@]}" "$@" -c "$SHIMS" -o "$obj"
    cc "${cflags[@]}" "$TEST_SRC" "${EXTRA_TUS[@]}" "$obj" -o "$bin" -lpthread
}

echo "=== phase-460 W6 -- Zephyr stack-slot release, host build ==="

EXTRA_TUS=()

for pool in "${POOL_SIZES[@]}"; do
    bin="$OUT/thread_slots_n${pool}"
    if ! build "$bin" "$pool" > "$OUT/build_n${pool}.log" 2>&1; then
        fail "N=$pool: the shims do not compile on the host"
        sed 's/^/      /' "$OUT/build_n${pool}.log"
        continue
    fi

    for case_name in n-plus-2 detach; do
        log="$OUT/${case_name}_n${pool}.log"
        if "$bin" "$case_name" > "$log" 2>&1; then
            pass "N=$pool $case_name"
            if [ "$VERBOSE" = true ]; then show "$log"; fi
        else
            fail "N=$pool $case_name"
            show "$log"
        fi
    done
done

# The mutant. `-Dnros_zephyr_task_slot_release=...` renames the REAL release
# inside the shims TU, and `thread_slot_leak_mutant.c` supplies a do-nothing
# definition under the original name. Nothing in the product is edited; the
# preprocessor does the damage, for one binary, in tmp/.
mutant_pool=4
mutant_bin="$OUT/thread_slots_mutant"
EXTRA_TUS=("$MUTANT_SRC")
if ! build "$mutant_bin" "$mutant_pool" \
        -Dnros_zephyr_task_slot_release=nros_zephyr_task_slot_release_defused \
        > "$OUT/build_mutant.log" 2>&1; then
    fail "the leaking-shim mutant does not build"
    sed 's/^/      /' "$OUT/build_mutant.log"
else
    if "$mutant_bin" n-plus-2 > "$OUT/mutant.log" 2>&1; then
        fail "the leaking-shim mutant PASSED n-plus-2 -- this gate has no teeth"
        sed 's/^/      /' "$OUT/mutant.log"
    else
        pass "the leaking-shim mutant fails n-plus-2, as it must"
        if [ "$VERBOSE" = true ]; then sed 's/^/      /' "$OUT/mutant.log"; fi
    fi
fi

echo
if [ "$failures" -ne 0 ]; then
    echo "zephyr thread-slot gate: $failures case(s) FAILED (logs in $OUT/)"
    exit 1
fi
printf 'zephyr thread-slot gate: all cases passed (N in %s plus the leak mutant)\n' \
    "$(IFS=,; echo "${POOL_SIZES[*]}")"
