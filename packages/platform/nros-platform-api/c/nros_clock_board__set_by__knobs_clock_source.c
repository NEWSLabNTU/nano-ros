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
#include <nros/platform.h>

/* true: *out_ns is ns since the Unix epoch. false: not synced. */
bool nros_board_wall_clock_ns(uint64_t* out_ns);

bool nros_clock_board_try__set_by__knobs_clock_source(uint32_t settle_ms);

/* Ask the board once and, when it answers, install the offset through the
 * ONE setter. Returns whether the offset was installed. */
static bool nros_clock_board_ask(void) {
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
    return nros_clock_set_epoch_offset_us(board_us - mono_us, NROS_WALL_CLOCK_SOURCE_BOARD);
}

/* `board` behaves like `sntp`: while nothing is latched, the board gets the
 * settle window (`clock.fallback_settle_ms`) to answer, asked every 10 ms on
 * the image's monotonic clock. The first answer installs and the resolver
 * latches `board`; a window that runs out is "not answered", and the
 * resolver tries the next listed source. 0 asks once. After resolution
 * nothing calls the hook again. */
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
