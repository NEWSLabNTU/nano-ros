#!/usr/bin/env bash
# phase-460 W7 -- an exhausted platform heap reaches the FATAL HOOK, on the HOST.
#
# Compiles the REAL `packages/platform/nros-platform-zephyr/src/platform.c`
# against the stub Zephyr tree in `tests/zephyr/host-platform/` and drives its
# allocation path from `tests/zephyr/heap_exhaustion_test.c`. See that file's
# header for what is under test, why it is testable here at all, and why it is
# not a native_sim app; see the stub tree's `kernel.h` for the rule those
# headers follow.
#
# No west, no Zephyr SDK and no board: `cc` and pthreads are the whole
# prerequisite, which is what puts this on the `just ci l1` gate lane rather
# than in the native_sim suite beside `run-c.sh`.
#
# TWO BUILDS, FOUR RUNS, and all four are the gate:
#
#   1. fatal      on the CONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL=y build -- the
#                 wave's gate proper: record, then line, then hook.
#   2. ok         on the same build -- a request that fits touches none of it.
#   3. not-fatal  on the knob-OFF build -- the negative control the phase's
#                 gate table names: NULL-and-log, and the record still written.
#   4. fatal      on the knob-OFF build, where it must FAIL. A gate nobody has
#                 seen fail is a gate nobody has measured, and this is the
#                 cheapest possible mutant: the SAME assertions against the
#                 build where the behaviour was compiled out.
#
# Usage:  ./tests/zephyr/run-heap-exhaustion.sh [--verbose]

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

PLATFORM="packages/platform/nros-platform-zephyr/src/platform.c"
TEST_SRC="tests/zephyr/heap_exhaustion_test.c"
STUBS="tests/zephyr/host-platform/include"
ABI="packages/platform/nros-platform-api/include"

# `test -f` on every input before compiling anything: a renamed or moved source
# must read as a FAILURE, not as a gate with nothing to check (issue 0196's
# class -- the same reason `zephyr-thread-slots` checks its TUs exist).
for f in "$PLATFORM" "$TEST_SRC" "$STUBS/zephyr/kernel.h" "$ABI/nros/platform.h"; do
    if [ ! -f "$f" ]; then
        echo "ERROR: $f is MISSING -- this gate would pass on absence." >&2
        exit 1
    fi
done

OUT="tmp/zephyr-heap-exhaustion"
mkdir -p "$OUT"

# CONFIG_NROS_BOOT_REPORT is on in BOTH builds: the record is the channel this
# wave exists to keep, so a case that did not write it would be testing a
# configuration the island does not ship. Everything left unset --
# CONFIG_POSIX_API, CONFIG_DYNAMIC_THREAD, CONFIG_SNTP, CONFIG_LOG,
# CONFIG_ARCH_POSIX, CONFIG_INIT_STACKS, CONFIG_TEST_RANDOM_GENERATOR --
# removes an arm of `platform.c` the stub tree would otherwise have to model.
common_cflags() {
    printf '%s\n' \
        -std=c11 -Wall -Wextra -Werror -D_GNU_SOURCE \
        -DCONFIG_NROS_BOOT_REPORT \
        "-I${STUBS}" \
        "-I${ABI}"
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

# The platform TU gets its own compile per knob setting, because the knob is
# what the two builds differ by and `IS_ENABLED` reads it at compile time.
build() {
    # build <binary> [extra cflags...]
    local bin="$1"
    shift
    local cflags=()
    mapfile -t cflags < <(common_cflags)
    local obj="${bin}.platform.o"
    cc "${cflags[@]}" "$@" -c "$PLATFORM" -o "$obj"
    cc "${cflags[@]}" "$@" "$TEST_SRC" "$obj" -o "$bin" -lpthread
}

echo "=== phase-460 W7 -- Zephyr heap exhaustion reaches the hook, host build ==="

fatal_bin="$OUT/heap_fatal"
plain_bin="$OUT/heap_not_fatal"

if ! build "$fatal_bin" -DCONFIG_NROS_HEAP_EXHAUSTION_IS_FATAL \
        > "$OUT/build_fatal.log" 2>&1; then
    fail "the knob-ON build does not compile on the host"
    sed 's/^/      /' "$OUT/build_fatal.log"
    exit 1
fi
if ! build "$plain_bin" > "$OUT/build_not_fatal.log" 2>&1; then
    fail "the knob-OFF build does not compile on the host"
    sed 's/^/      /' "$OUT/build_not_fatal.log"
    exit 1
fi

run_case() {
    # run_case <label> <binary> <case>
    local label="$1" bin="$2" name="$3"
    local log="$OUT/${label}.log"
    if "$bin" "$name" > "$log" 2>&1; then
        pass "$label"
        if [ "$VERBOSE" = true ]; then show "$log"; fi
    else
        fail "$label"
        show "$log"
    fi
}

run_case "knob on: exhaustion reaches the hook" "$fatal_bin" fatal
run_case "knob on: a request that fits is untouched" "$fatal_bin" ok
run_case "knob off: NULL-and-log, record still written" "$plain_bin" not-fatal

# The mutant: the gate's own assertions against the build where the behaviour
# was compiled out. It must FAIL.
if "$plain_bin" fatal > "$OUT/mutant.log" 2>&1; then
    fail "the knob-OFF build PASSED the fatal case -- this gate has no teeth"
    sed 's/^/      /' "$OUT/mutant.log"
else
    pass "the knob-OFF build fails the fatal case, as it must"
    if [ "$VERBOSE" = true ]; then sed 's/^/      /' "$OUT/mutant.log"; fi
fi

echo
if [ "$failures" -ne 0 ]; then
    echo "zephyr heap-exhaustion gate: $failures case(s) FAILED (logs in $OUT/)"
    exit 1
fi
echo "zephyr heap-exhaustion gate: all cases passed (knob on, knob off, and the knob-off mutant)"
