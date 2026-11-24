#!/usr/bin/env bash
# RFC-0141 D1/D4 / phase-582 W1c: boot resolution of the wall-clock source
# (`nros/clock_resolve.h`) and the policy it latches into the one epoch store
# (`nros/clock_epoch.h`), against a FAKE platform whose monotonic clock the
# test moves by hand, so every bound below is exact rather than a guess about
# scheduling.
#
# Cases, each bounded on BOTH sides:
#   1. one source that answers is latched; the reading is offset + monotonic;
#   2. a fallback ["sntp", "platform"] whose first source never answers gives
#      it AT LEAST its settle time (and no more than one poll step over), then
#      resolves to `platform` before anything is stamped;
#   3. while resolving, only the source being tried may install: a reading
#      from another source in the meantime is refused;
#   4. upgrade = never: once latched, an install by any other source is
#      refused, so no stamp in the run comes from two clocks;
#   5. upgrade = monotonic: a source EARLIER in the list takes over (one step,
#      counted); one LATER in the list is refused; and when the new source is
#      BEHIND the old one, readings hold at the last value returned until it
#      catches up, then ADVANCE with the monotonic clock;
#   6. epoch_failure = unstamped (the default): no source answered, the
#      resolver says so once, readings are 0 and the state is unsynced; a later
#      install by a listed source is a counted forward step;
#   7. epoch_failure = halt: no source answered, the image stops through
#      nros_platform_panic, with a message naming every source;
#   8. an empty list reads as ["platform"];
#   9. the knobs' comma-joined spelling parses, in order, whole words only;
#  10. SNTP re-sync pacing (issue 2884): on the server's clock by the rate two
#      answers measured, so a racing guest's 10 s interval is 10 s of the
#      server's, plus a 1 % guard band, doubled while the rate's window is
#      shorter than one interval (the FVP's measured boot window); monotonic
#      pacing before two answers;
#  11. a rate that cannot be trusted (a server step either way, no monotonic
#      time between answers, outside the bounds) paces that gap on monotonic
#      time.
# The negative control rebuilds the same test with the elapsed time between
# readings FROZEN: both "advances" cases must fail, so neither "never goes
# backwards" nor "holds" can pass on a clock that never moves.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$root/scripts/lib/grep-q.sh"
# shellcheck source=scripts/lib/deadline.sh
. "$root/scripts/lib/deadline.sh"
cc="${CC:-cc}"
command -v "$cc" >/dev/null || { echo "clock-resolve: no C compiler ($cc)" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
inc="$root/packages/platform/nros-platform-api/include"

cat > "$tmp/main.c" <<'EOF'
#include <nros/clock_resolve.h>
#include <setjmp.h>
#include <stdio.h>
#include <string.h>

/* ---- the fake platform ---- */
static uint64_t fake_ns = 5ULL * 1000000000ULL; /* boot + 5 s */
static void advance_ms(uint64_t ms) {
#ifndef FROZEN
    fake_ns += ms * 1000000ULL;
#else
    (void)ms;
#endif
}
uint64_t nros_platform_clock_ns(void) { return fake_ns; }
/* A poll's sleep always moves time, so a settle wait ends even in the frozen
 * build; what the frozen build freezes is the elapsed time the TEST steps
 * between readings (advance_ms). */
void nros_platform_sleep_ms(size_t ms) { fake_ns += (uint64_t)ms * 1000000ULL; }
uint32_t nros_platform_critical_section_acquire(void) { return 0; }
void nros_platform_critical_section_release(uint32_t t) { (void)t; }
static jmp_buf halted;
static char halt_msg[200];
void nros_platform_panic(const char* msg, size_t len) {
    size_t n = len < sizeof halt_msg - 1 ? len : sizeof halt_msg - 1;
    memcpy(halt_msg, msg, n);
    halt_msg[n] = '\0';
    longjmp(halted, 1);
}

/* ---- fake sources ---- */
#define OFFSET 1700000000000000ULL /* 2023-11-14, in us */
typedef struct {
    bool answers[4];        /* per NROS_WALL_CLOCK_SOURCE_* */
    uint64_t offset[4];
    bool tried[4];
    uint64_t waited_ms[4];
    bool foreign_admitted;  /* case 3: did another source's install get in? */
} script_t;
static char last_report[200];
static int reports;
static void report(const char* msg, size_t len, void* ctx) {
    (void)ctx;
    size_t n = len < sizeof last_report - 1 ? len : sizeof last_report - 1;
    memcpy(last_report, msg, n);
    last_report[n] = '\0';
    reports++;
}
static bool try_source(uint8_t s, uint32_t settle_ms, void* ctx) {
    script_t* sc = (script_t*)ctx;
    sc->tried[s] = true;
    if (s == NROS_WALL_CLOCK_SOURCE_PLATFORM) {
        return sc->answers[s];
    }
    /* A reading from ANOTHER source lands while this one is tried. */
    uint8_t other = s == NROS_WALL_CLOCK_SOURCE_SNTP ? NROS_WALL_CLOCK_SOURCE_ZENOH_HLC
                                                     : NROS_WALL_CLOCK_SOURCE_SNTP;
    if (nros_clock_set_epoch_offset_us(OFFSET + 999ULL, other)) {
        sc->foreign_admitted = true;
    }
    uint64_t t0 = nros_platform_clock_ns();
    if (sc->answers[s]) {
        advance_ms(40); /* an exchange takes a while */
        nros_clock_set_epoch_offset_us(sc->offset[s], s);
    }
    bool ok = nros_clock_wait_synced(settle_ms);
    sc->waited_ms[s] = (nros_platform_clock_ns() - t0) / 1000000ULL;
    return ok;
}

static void reset(void) {
    memset(&nros_clock_epoch_store, 0, sizeof nros_clock_epoch_store);
    reports = 0;
    last_report[0] = '\0';
    halt_msg[0] = '\0';
}
static uint64_t mono_us(void) { return nros_platform_clock_ns() / 1000ULL; }

static int bad;
#define CHECK(name, cond) do { if (cond) printf("  ok   %s\n", name); \
    else { printf("  FAIL %s\n", name); bad = 1; } } while (0)

int main(void) {
    const uint8_t sntp[] = {NROS_WALL_CLOCK_SOURCE_SNTP};
    const uint8_t sntp_platform[] = {NROS_WALL_CLOCK_SOURCE_SNTP, NROS_WALL_CLOCK_SOURCE_PLATFORM};
    const uint8_t sntp_hlc[] = {NROS_WALL_CLOCK_SOURCE_SNTP, NROS_WALL_CLOCK_SOURCE_ZENOH_HLC};

    /* 1. one source that answers */
    reset();
    script_t sc = {0};
    sc.answers[NROS_WALL_CLOCK_SOURCE_SNTP] = true;
    sc.offset[NROS_WALL_CLOCK_SOURCE_SNTP] = OFFSET;
    nros_clock_policy_t p = {sntp, 1, 1000, NROS_CLOCK_UPGRADE_NEVER, NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED};
    uint8_t got = nros_clock_resolve(&p, try_source, report, &sc);
    CHECK("1 one source: latched sntp", got == NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("1 one source: reported once, naming it",
          reports == 1 && strstr(last_report, "wall clock from sntp") != NULL);
    uint64_t m = mono_us();
    uint64_t e = nros_clock_epoch_us();
    CHECK("1 reading == offset + monotonic", e == OFFSET + m);
    nros_wall_clock_state_t st = nros_platform_wall_clock_state_fake();
    (void)st;

    /* 2. fallback, first source silent */
    reset();
    memset(&sc, 0, sizeof sc);
    sc.answers[NROS_WALL_CLOCK_SOURCE_PLATFORM] = true;
    p = (nros_clock_policy_t){sntp_platform, 2, 1000, NROS_CLOCK_UPGRADE_NEVER,
                              NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED};
    got = nros_clock_resolve(&p, try_source, report, &sc);
    CHECK("2 fallback: resolves to platform", got == NROS_WALL_CLOCK_SOURCE_PLATFORM);
    CHECK("2 fallback: sntp waited at least its settle time",
          sc.waited_ms[NROS_WALL_CLOCK_SOURCE_SNTP] >= 1000ULL);
    CHECK("2 fallback: and no more than one poll step over",
          sc.waited_ms[NROS_WALL_CLOCK_SOURCE_SNTP] <= 1010ULL);
    st = nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("2 state: names platform, not the configured sntp",
          st.source == NROS_WALL_CLOCK_SOURCE_PLATFORM);
    CHECK("2 state: no steps", st.steps == 0u);

    /* 3. while resolving, only the source being tried installs */
    CHECK("3 a foreign source's reading during resolution was refused", !sc.foreign_admitted);

    /* 4. upgrade = never: sntp arriving later is refused */
    CHECK("4 never: a later sntp install is refused",
          !nros_clock_set_epoch_offset_us(OFFSET, NROS_WALL_CLOCK_SOURCE_SNTP));
    CHECK("4 never: the reading stays the platform's (0 on a port with none)",
          nros_clock_epoch_us() == 0ULL);
    CHECK("4 never: no step", nros_clock_epoch_state(0).steps == 0u);

    /* 5. upgrade = monotonic: hlc latched, sntp (earlier) takes over BEHIND it */
    reset();
    memset(&sc, 0, sizeof sc);
    sc.answers[NROS_WALL_CLOCK_SOURCE_ZENOH_HLC] = true;
    sc.offset[NROS_WALL_CLOCK_SOURCE_ZENOH_HLC] = OFFSET;
    p = (nros_clock_policy_t){sntp_hlc, 2, 500, NROS_CLOCK_UPGRADE_MONOTONIC,
                              NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED};
    got = nros_clock_resolve(&p, try_source, report, &sc);
    CHECK("5 monotonic: resolves to zenoh-hlc", got == NROS_WALL_CLOCK_SOURCE_ZENOH_HLC);
    uint64_t r0 = nros_clock_epoch_us();
    advance_ms(1000);
    uint64_t r1 = nros_clock_epoch_us();
    CHECK("5 latched clock advances (a frozen clock fails here)", r1 >= r0 + 1000000ULL);
    CHECK("5 latched clock advances by no more than elapsed", r1 <= r0 + 1000000ULL);
    CHECK("5 a source LATER in the list is refused",
          !nros_clock_set_epoch_offset_us(OFFSET + 1ULL, NROS_WALL_CLOCK_SOURCE_PLATFORM));
    /* sntp syncs 5 s BEHIND the hlc offset */
    CHECK("5 a source EARLIER in the list takes over",
          nros_clock_set_epoch_offset_us(OFFSET - 5000000ULL, NROS_WALL_CLOCK_SOURCE_SNTP));
    st = nros_clock_epoch_state(0);
    CHECK("5 the take-over is one counted step", st.steps == 1u);
    CHECK("5 the state names the new source", st.source == NROS_WALL_CLOCK_SOURCE_SNTP);
    advance_ms(1000);
    uint64_t r2 = nros_clock_epoch_us();
    CHECK("5 behind: the reading never goes backwards", r2 >= r1);
    CHECK("5 behind: it holds at the last value instead of stepping", r2 == r1);
    advance_ms(6000);
    uint64_t r3 = nros_clock_epoch_us();
    CHECK("5 caught up: it ADVANCES past the hold (a frozen clock fails here)", r3 > r1);
    CHECK("5 caught up: by exactly the new source's reading",
          r3 == OFFSET - 5000000ULL + mono_us());
    CHECK("5 the latched source cannot be displaced downward afterwards",
          !nros_clock_set_epoch_offset_us(OFFSET, NROS_WALL_CLOCK_SOURCE_ZENOH_HLC));

    /* 6. unstamped */
    reset();
    memset(&sc, 0, sizeof sc);
    p = (nros_clock_policy_t){sntp, 1, 300, NROS_CLOCK_UPGRADE_NEVER, NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED};
    got = nros_clock_resolve(&p, try_source, report, &sc);
    CHECK("6 unstamped: no source latched", got == NROS_CLOCK_SOURCE_NONE);
    CHECK("6 unstamped: said so once",
          reports == 1 && strstr(last_report, "no wall-clock source answered (sntp)") != NULL &&
              strstr(last_report, "stamps are unset") != NULL);
    CHECK("6 unstamped: reading is 0", nros_clock_epoch_us() == 0ULL);
    st = nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("6 unstamped: state unsynced", !st.synced);
    CHECK("6 a later sntp install is admitted",
          nros_clock_set_epoch_offset_us(OFFSET, NROS_WALL_CLOCK_SOURCE_SNTP));
    st = nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_SNTP);
    CHECK("6 and is a counted forward step from unset", st.synced && st.steps == 1u);
    CHECK("6 an unlisted source is still refused",
          !nros_clock_set_epoch_offset_us(OFFSET, NROS_WALL_CLOCK_SOURCE_BOARD));

    /* 7. halt */
    reset();
    memset(&sc, 0, sizeof sc);
    p = (nros_clock_policy_t){sntp_hlc, 2, 200, NROS_CLOCK_UPGRADE_NEVER, NROS_CLOCK_EPOCH_FAILURE_HALT};
    if (setjmp(halted) == 0) {
        (void)nros_clock_resolve(&p, try_source, report, &sc);
        CHECK("7 halt: the image stopped", false);
    } else {
        CHECK("7 halt: the image stopped", true);
        CHECK("7 halt: the message names every source",
              strstr(halt_msg, "(sntp, zenoh-hlc)") != NULL);
        CHECK("7 halt: and the knob", strstr(halt_msg, "clock.epoch_failure = halt") != NULL);
        CHECK("7 halt: both sources were tried",
              sc.tried[NROS_WALL_CLOCK_SOURCE_SNTP] && sc.tried[NROS_WALL_CLOCK_SOURCE_ZENOH_HLC]);
    }

    /* 9. the knobs' spelling */
    uint8_t ls[NROS_CLOCK_MAX_SOURCES];
    uint8_t ln = nros_clock_parse_sources("sntp, zenoh-hlc,platform", ls);
    CHECK("9 parse: three sources, in order", ln == 3u && ls[0] == NROS_WALL_CLOCK_SOURCE_SNTP &&
              ls[1] == NROS_WALL_CLOCK_SOURCE_ZENOH_HLC && ls[2] == NROS_WALL_CLOCK_SOURCE_PLATFORM);
    CHECK("9 parse: one source", nros_clock_parse_sources("board", ls) == 1u &&
              ls[0] == NROS_WALL_CLOCK_SOURCE_BOARD);
    CHECK("9 parse: a prefix is not a source", nros_clock_parse_sources("sntpx,sn", ls) == 0u);
    CHECK("9 parse: empty", nros_clock_parse_sources("", ls) == 0u);
    CHECK("9 upgrade", nros_clock_parse_upgrade("monotonic") == NROS_CLOCK_UPGRADE_MONOTONIC &&
              nros_clock_parse_upgrade("never") == NROS_CLOCK_UPGRADE_NEVER);
    CHECK("9 epoch_failure", nros_clock_parse_epoch_failure("halt") == NROS_CLOCK_EPOCH_FAILURE_HALT &&
              nros_clock_parse_epoch_failure("unstamped") == NROS_CLOCK_EPOCH_FAILURE_UNSTAMPED);

    /* 8. empty list */
    reset();
    memset(&sc, 0, sizeof sc);
    sc.answers[NROS_WALL_CLOCK_SOURCE_PLATFORM] = true;
    p = (nros_clock_policy_t){NULL, 0, 100, NROS_CLOCK_UPGRADE_NEVER, NROS_CLOCK_EPOCH_FAILURE_HALT};
    got = nros_clock_resolve(&p, try_source, report, &sc);
    CHECK("8 empty list: platform", got == NROS_WALL_CLOCK_SOURCE_PLATFORM);

    /* 10. SNTP re-sync pacing (issue 2884): on the server's clock, by its rate. */
    {
        uint64_t r = 0;
        nros_sntp_pace_t sp = {0};
        CHECK("10 no answer: rate unknown",
              nros_sntp_pace_rate(&sp, &r) == NROS_SNTP_PACE_RATE_UNKNOWN);
        CHECK("10 no answer: paced on monotonic time, due at the interval, not before",
              !nros_sntp_pace_due(&sp, 9999, 10000) && nros_sntp_pace_due(&sp, 10000, 10000));
        nros_sntp_pace_answer(&sp, 1000, 1700000000000ULL);
        CHECK("10 one answer (the boot sync): still monotonic pacing",
              nros_sntp_pace_rate(&sp, &r) == NROS_SNTP_PACE_RATE_UNKNOWN &&
                  nros_sntp_pace_due(&sp, 10000, 10000));
        /* A guest racing 10x: 100 s of guest time was 10 s of the server's,
         * a window of one full interval. */
        nros_sntp_pace_answer(&sp, 101000, 1700000010000ULL);
        CHECK("10 racing guest: rate 0.1", nros_sntp_pace_rate(&sp, &r) == NROS_SNTP_PACE_RATE_OK &&
                                               r == 100000ULL);
        CHECK("10 racing guest: a 10 s interval waits 101 s of guest time (10 s real + 1 %)",
              nros_sntp_pace_window_full(&sp, 10000) && !nros_sntp_pace_due(&sp, 100999, 10000) &&
                  nros_sntp_pace_due(&sp, 101000, 10000));
        /* Hardware: the two clocks agree. */
        nros_sntp_pace_t hw = {0};
        nros_sntp_pace_answer(&hw, 0, 1700000000000ULL);
        nros_sntp_pace_answer(&hw, 60000, 1700000060000ULL);
        CHECK("10 hardware: rate 1, due at the interval + 1 %",
              nros_sntp_pace_rate(&hw, &r) == NROS_SNTP_PACE_RATE_OK && r == 1000000ULL &&
                  !nros_sntp_pace_due(&hw, 60599, 60000) && nros_sntp_pace_due(&hw, 60600, 60000));
        /* The FVP's boot window (measured): 4.5 s of the server's over 10 s of
         * guest time, shorter than the 10 s interval, so the next deadline is
         * doubled; at the guest's steady 3.64x it is ~12 s of host time where
         * the undoubled one was the measured 6.1 s. */
        nros_sntp_pace_t boot = {0};
        nros_sntp_pace_answer(&boot, 5000, 1700000000000ULL);
        nros_sntp_pace_answer(&boot, 15000, 1700000004524ULL);
        CHECK("10 a short window: rate known, window not full",
              nros_sntp_pace_rate(&boot, &r) == NROS_SNTP_PACE_RATE_OK && r == 452400ULL &&
                  !nros_sntp_pace_window_full(&boot, 10000));
        CHECK("10 a short window doubles the deadline (+1 %): 44.65 s of guest time",
              !nros_sntp_pace_due(&boot, 44650, 10000) && nros_sntp_pace_due(&boot, 44651, 10000));
        CHECK("10 ...which at 3.64x is at least 12 s of host time", 44651ULL * 100 / 364 >= 12000ULL);

        /* 11. a rate that cannot be trusted falls back to monotonic pacing. */
        nros_sntp_pace_t st = {0};
        nros_sntp_pace_answer(&st, 0, 1700000000000ULL);
        nros_sntp_pace_answer(&st, 10000, 1700003600000ULL); /* the server stepped +1 h */
        CHECK("11 a server step forward is out of range",
              nros_sntp_pace_rate(&st, &r) == NROS_SNTP_PACE_RATE_OUT_OF_RANGE && r == 0u);
        CHECK("11 ... and that gap is paced on monotonic time",
              !nros_sntp_pace_due(&st, 9999, 10000) && nros_sntp_pace_due(&st, 10000, 10000));
        nros_sntp_pace_t back = {0};
        nros_sntp_pace_answer(&back, 0, 1700000060000ULL);
        nros_sntp_pace_answer(&back, 10000, 1700000000000ULL); /* stepped back */
        CHECK("11 a server step back is out of range",
              nros_sntp_pace_rate(&back, &r) == NROS_SNTP_PACE_RATE_OUT_OF_RANGE);
        nros_sntp_pace_t zero = {0};
        nros_sntp_pace_answer(&zero, 5000, 1700000000000ULL);
        nros_sntp_pace_answer(&zero, 5000, 1700000001000ULL); /* no monotonic time */
        CHECK("11 a zero monotonic delta is out of range",
              nros_sntp_pace_rate(&zero, &r) == NROS_SNTP_PACE_RATE_OUT_OF_RANGE &&
                  nros_sntp_pace_due(&zero, 10000, 10000));
        nros_sntp_pace_t slow = {0};
        nros_sntp_pace_answer(&slow, 0, 1700000000000ULL);
        nros_sntp_pace_answer(&slow, 10000000, 1700000005000ULL); /* rate 0.0005 */
        CHECK("11 a rate below the bound is out of range",
              nros_sntp_pace_rate(&slow, &r) == NROS_SNTP_PACE_RATE_OUT_OF_RANGE);
        nros_sntp_pace_t edge = {0};
        nros_sntp_pace_answer(&edge, 0, 1700000000000ULL);
        nros_sntp_pace_answer(&edge, 10000, 1700001000000ULL); /* rate 100, the bound */
        CHECK("11 the upper bound itself is usable",
              nros_sntp_pace_rate(&edge, &r) == NROS_SNTP_PACE_RATE_OK && r == 100000000ULL);
    }
    return bad;
}
EOF
# The state accessor case 1 names is the port's; this test has no port, so it
# reads the store directly.
sed -i 's/nros_platform_wall_clock_state_fake()/nros_clock_epoch_state(NROS_WALL_CLOCK_SOURCE_SNTP)/' "$tmp/main.c"

build() { # <out> [extra cflags]
    local out="$1"; shift
    "$cc" -std=c11 -Wall -Wextra -Werror -I"$inc" "$@" "$tmp/main.c" -o "$out" 2>"$out.err"
}

if ! build "$tmp/real"; then
    echo "clock-resolve: the test did not build:" >&2
    cat "$tmp/real.err" >&2
    exit 1
fi
echo "clock-resolve: against a fake platform"
if ! "$tmp/real"; then
    echo "clock-resolve: FAILED" >&2
    exit 1
fi

if ! build "$tmp/frozen" -DFROZEN; then
    echo "clock-resolve: the negative control did not build:" >&2
    cat "$tmp/frozen.err" >&2
    exit 1
fi
rc=0; out="$("${NROS_DEADLINE[@]}" 20 "$tmp/frozen")" || rc=$?
if [ "$rc" -eq 0 ]; then
    echo "clock-resolve: negative control: a FROZEN clock passed every case" >&2
    printf '%s\n' "$out" >&2
    exit 1
fi
for want in 'FAIL 5 latched clock advances (' 'FAIL 5 caught up: it ADVANCES'; do
    nros_grep_q -F "$want" <<<"$out"; m=$?
    if [ "$m" -ge 2 ]; then
        echo "clock-resolve: grep failed reading the negative control's output" >&2
        exit 1
    fi
    if [ "$m" -ne 0 ]; then
        echo "clock-resolve: negative control: a FROZEN clock passed a case it must fail: $want" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
done
echo "clock-resolve: OK (negative control: a frozen clock fails both advance cases)"
