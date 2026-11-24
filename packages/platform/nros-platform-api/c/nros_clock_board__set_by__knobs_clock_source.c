/* RFC-0141 D2 (phase-582 W1d) — the `board` wall-clock source.
 *
 * Compiled ONLY when the image's `[knobs.clock] source` names `board`; the
 * build script adds this file and `NROS_CLOCK_HAS_BOARD` together
 * (`nros_zephyr_build::clock_selection`). Its name is the selection's on
 * purpose: when the board does not define the hook, the linker's
 * "undefined reference to `nros_board_wall_clock_ns'" comes from an object
 * whose name says which knob demanded it.
 *
 * ONE file for every port that resolves `board` (POSIX, FreeRTOS): it needs
 * only the platform ABI's monotonic clock and sleep, and the one setter.
 *
 * The hook is DECLARED here and defined nowhere in nano-ros: no default is
 * compiled, because a strong definition in an archive never replaces a weak
 * default that is already linked (RFC-0141 D2). The board supplies it as an
 * object of the image, or in an archive linked after nano-ros. */
#include <stdbool.h>
#include <stdint.h>

#include <nros/clock_epoch.h>
#include <nros/clock_resync.h>
#include <nros/platform.h>

/* true: *out_ns is ns since the Unix epoch. false: not synced. */
bool nros_board_wall_clock_ns(uint64_t* out_ns);

bool nros_clock_board_try__set_by__knobs_clock_source(uint32_t settle_ms);
void nros_clock_board_resync_start__set_by__knobs_clock_source(void);

/* Ask the board once. Returns whether it answered with a usable instant,
 * and the offset that instant gives. */
static bool nros_clock_board_answer(uint64_t* offset_us) {
    uint64_t board_ns = 0ULL;
    if (!nros_board_wall_clock_ns(&board_ns)) {
        return false;
    }
    const uint64_t board_us = board_ns / 1000ULL;
    const uint64_t mono_us = nros_platform_clock_ns() / 1000ULL;
    /* An instant at or before boot is no Unix time this image can stamp
     * with, and its offset would wrap; treat it as "not synced". */
    if (board_us <= mono_us) {
        return false;
    }
    *offset_us = board_us - mono_us;
    return true;
}

/* Ask the board once and, when it answers, install the offset through the
 * ONE setter. Returns whether the offset was installed. */
static bool nros_clock_board_ask(void) {
    uint64_t offset_us = 0ULL;
    return nros_clock_board_answer(&offset_us) &&
           nros_clock_set_epoch_offset_us(offset_us, NROS_WALL_CLOCK_SOURCE_BOARD);
}

/* `board` behaves like `sntp`: while nothing is latched, the board gets the
 * settle window (`clock.fallback_settle_ms`) to answer, asked every 10 ms on
 * the image's monotonic clock. The first answer installs and the resolver
 * latches `board`; a window that runs out is "not answered", and the
 * resolver tries the next listed source. 0 asks once. After resolution only
 * the re-ask below calls the hook again. */
bool nros_clock_board_try__set_by__knobs_clock_source(uint32_t settle_ms) {
    const uint64_t start_ns = nros_platform_clock_ns();
    const uint64_t budget_ns = (uint64_t)settle_ms * 1000000ULL;
    for (;;) {
        if (nros_clock_board_ask()) {
            return true;
        }
        if (nros_platform_clock_ns() - start_ns >= budget_ns) {
            return false;
        }
        nros_platform_sleep_ms(10);
    }
}

/* `clock.board_resync_interval_ms` (default 0: never asked again) and
 * `clock.board_resync_stack_bytes`, delivered as defines by the image's clock
 * selection. The defaults are the registry's. */
#ifndef NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS
#define NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS 0u
#endif
#ifndef NROS_CLOCK_BOARD_RESYNC_STACK_BYTES
#define NROS_CLOCK_BOARD_RESYNC_STACK_BYTES 8192u
#endif

/* Severity 3 (warn) on a failure, 2 (info) otherwise; no logger name, the
 * line already says `nros:`. */
static void nros_clock_board_log(uint8_t severity, const char* msg) {
    uintptr_t n = 0;
    while (msg[n] != '\0') {
        n++;
    }
    nros_platform_log_write(severity, (const uint8_t*)"", 0, (const uint8_t*)msg, n);
}

/* A re-ask "answered" when the HOOK answered. Whether the setter then
 * admits the offset is `clock.upgrade`'s decision (another source may be
 * latched), and a refusal there is policy, not a board that went quiet. */
static bool nros_clock_board_reask(void* ctx) {
    (void)ctx;
    uint64_t offset_us = 0ULL;
    if (!nros_clock_board_answer(&offset_us)) {
        return false;
    }
    (void)nros_clock_set_epoch_offset_us(offset_us, NROS_WALL_CLOCK_SOURCE_BOARD);
    return true;
}

static void nros_clock_board_reask_changed(void* ctx, bool answered) {
    (void)ctx;
    /* Only a change after the first ask is news: a hook that answers every
     * time says nothing at all. */
    static bool s_reported_failure = false;
    if (!answered) {
        nros_clock_board_log(3u, "nros: board wall clock did not answer a re-ask; keeping the "
                                 "installed offset");
        s_reported_failure = true;
    } else if (s_reported_failure) {
        nros_clock_board_log(2u, "nros: board wall clock answering again");
    }
}

static nros_clock_resync_t s_board_resync;

/* Called by the port once the resolution is latched, when `board` is among
 * the image's sources. With the interval at 0 it starts nothing. The interval
 * is the image's monotonic time on purpose: the hook is the board's own code,
 * so there is no remote rate to keep real (the SNTP re-sync paces on the
 * server's clock for that reason, and this does not). */
void nros_clock_board_resync_start__set_by__knobs_clock_source(void) {
    if ((uint32_t)(NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS) == 0u) {
        return;
    }
    s_board_resync.name = "nros_board_clock";
    s_board_resync.interval_ms = (uint32_t)(NROS_CLOCK_BOARD_RESYNC_INTERVAL_MS);
    s_board_resync.stack_bytes = (size_t)(NROS_CLOCK_BOARD_RESYNC_STACK_BYTES);
    s_board_resync.ask = nros_clock_board_reask;
    s_board_resync.changed = nros_clock_board_reask_changed;
    if (nros_clock_resync_start(&s_board_resync) != 0) {
        nros_clock_board_log(3u, "nros: board wall clock re-ask task could not start; the "
                                 "board is asked once");
    }
}
