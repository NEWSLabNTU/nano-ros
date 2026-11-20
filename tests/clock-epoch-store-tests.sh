#!/usr/bin/env bash
# RFC-0141 D3/D4 / phase-582 W1a, W1b: the ONE wall-clock epoch store
# (`packages/platform/nros-platform-api/include/nros/clock_epoch.h`), against
# the POSIX port's monotonic clock and critical section.
#
# Cases, each with a bound on BOTH sides, so a clock that never moves cannot
# pass a "never goes backwards" or "reads the offset" case:
#   1. unset: the offset and the wall reading are 0 ("no wall clock");
#   2. after a set: offset + mono-before <= reading <= offset + mono-after;
#   3. advance: two readings 20 ms apart differ by at least 20 ms (and by
#      less than a generous ceiling);
#   4. one store across translation units: a set in one TU is what a read in
#      another TU sees, which is what the weak definition must guarantee;
#   5. the state (W1b): unsynced and naming the configured source before the
#      first install; synced on the installing source after it; a re-install
#      by the SAME source is not a step, one by a DIFFERENT source is exactly
#      one, and the state then names the new source.
# The negative control builds the same test against a FROZEN monotonic clock
# and requires case 3 to fail; without it, case 3 could be vacuous.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$root/scripts/lib/grep-q.sh"
cc="${CC:-cc}"
command -v "$cc" >/dev/null || { echo "clock-epoch-store: no C compiler ($cc)" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
inc="$root/packages/platform/nros-platform-api/include"

cat > "$tmp/writer.c" <<'EOF'
#include <nros/clock_epoch.h>
void writer_set(uint64_t offset_us) {
    nros_clock_set_epoch_offset_us(offset_us, NROS_WALL_CLOCK_SOURCE_SNTP);
}
EOF

cat > "$tmp/main.c" <<'EOF'
#include <nros/clock_epoch.h>
#include <stdio.h>
#include <time.h>
void writer_set(uint64_t offset_us);
static int bad;
#define CHECK(name, cond) do { if (cond) printf("  ok   %s\n", name); \
    else { printf("  FAIL %s\n", name); bad = 1; } } while (0)
static uint64_t mono_us(void) { return nros_platform_clock_ns() / 1000ULL; }
int main(void) {
    CHECK("unset: offset is 0", nros_clock_epoch_offset_us() == 0ULL);
    CHECK("unset: wall reading is 0 (no wall clock)", nros_clock_epoch_us() == 0ULL);
    nros_wall_clock_state_t st = nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("state unset: not synced", !st.synced);
    CHECK("state unset: names the configured source", st.source == NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("state unset: no steps", st.steps == 0u);

    const uint64_t offset = 1700000000000000ULL; /* 2023-11-14, in us */
    nros_clock_set_epoch_offset_us(offset, NROS_WALL_CLOCK_SOURCE_SNTP);
    st = nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_PLATFORM);
    CHECK("state set: synced", st.synced);
    CHECK("state set: names the installing source, not the configured one",
          st.source == NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("state set: the first install is not a step", st.steps == 0u);
    uint64_t m0 = mono_us();
    uint64_t e = nros_clock_epoch_us();
    uint64_t m1 = mono_us();
    CHECK("set: offset reads back", nros_clock_epoch_offset_us() == offset);
    CHECK("set: reading >= offset + monotonic before", e >= offset + m0);
    CHECK("set: reading <= offset + monotonic after", e <= offset + m1);

    uint64_t e1 = nros_clock_epoch_us();
    struct timespec d = {0, 20 * 1000 * 1000};
    nanosleep(&d, NULL);
    uint64_t e2 = nros_clock_epoch_us();
    CHECK("advance: at least the 20 ms slept (a frozen clock fails here)", e2 >= e1 + 20000ULL);
    CHECK("advance: under 5 s", e2 < e1 + 5000000ULL);

    writer_set(offset + 7ULL);
    CHECK("one store: another TU's set is this TU's read", nros_clock_epoch_offset_us() == offset + 7ULL);
    CHECK("state: a re-install by the same source is not a step",
          nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_PLATFORM).steps == 0u);

    nros_clock_set_epoch_offset_us(offset + 9ULL, NROS_WALL_CLOCK_SOURCE_ZENOH_HLC);
    st = nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_PLATFORM);
    CHECK("state: an install by a different source is exactly one step", st.steps == 1u);
    CHECK("state: names the new source", st.source == NROS_WALL_CLOCK_SOURCE_ZENOH_HLC);
    CHECK("state: still synced", st.synced);
    return bad;
}
EOF

# A frozen monotonic clock for the negative control: the POSIX port's own
# definition is replaced by one that never moves.
cat > "$tmp/frozen.c" <<'EOF'
#include <stdint.h>
uint64_t nros_platform_clock_ns(void) { return 123456789ULL; }
uint32_t nros_platform_critical_section_acquire(void) { return 0; }
void nros_platform_critical_section_release(uint32_t t) { (void)t; }
EOF

build() { # <out> <port sources...>
    local out="$1"; shift
    "$cc" -std=c11 -Wall -Wextra -Werror -D_POSIX_C_SOURCE=200809L -I"$inc" \
        "$tmp/main.c" "$tmp/writer.c" "$@" -o "$out" -lpthread 2>"$out.err"
}

if ! build "$tmp/real" "$root/packages/platform/nros-platform-posix/src/platform.c" \
        "$root/packages/platform/nros-platform-posix/src/termination.c"; then
    echo "clock-epoch-store: the test did not build against the POSIX port:" >&2
    cat "$tmp/real.err" >&2
    exit 1
fi
echo "clock-epoch-store: against the POSIX port"
if ! "$tmp/real"; then
    echo "clock-epoch-store: FAILED" >&2
    exit 1
fi

if ! build "$tmp/frozen" "$tmp/frozen.c"; then
    echo "clock-epoch-store: the negative control did not build:" >&2
    cat "$tmp/frozen.err" >&2
    exit 1
fi
rc=0; out="$("$tmp/frozen")" || rc=$?
if [ "$rc" -eq 0 ]; then
    echo "clock-epoch-store: negative control: a FROZEN clock passed every case" >&2
    printf '%s\n' "$out" >&2
    exit 1
fi
nros_grep_q 'FAIL advance: at least' <<<"$out"; m=$?
if [ "$m" -ge 2 ]; then
    echo "clock-epoch-store: grep failed reading the negative control's output" >&2
    exit 1
fi
if [ "$m" -ne 0 ]; then
    echo "clock-epoch-store: negative control: a FROZEN clock passed the advance case" >&2
    printf '%s\n' "$out" >&2
    exit 1
fi
echo "clock-epoch-store: OK (negative control: a frozen clock fails the advance case)"
