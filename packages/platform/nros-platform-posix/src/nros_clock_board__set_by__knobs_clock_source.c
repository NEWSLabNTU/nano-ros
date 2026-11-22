/* RFC-0141 D2 (phase-582 W1d) — the `board` wall-clock source.
 *
 * Compiled ONLY when the image's `[knobs.clock] source` names `board`; the
 * build script adds this file and `NROS_CLOCK_HAS_BOARD` together
 * (`nros_zephyr_build::clock_selection`). Its name is the selection's on
 * purpose: when the board does not define the hook, the linker's
 * "undefined reference to `nros_board_wall_clock_ns'" comes from an object
 * whose name says which knob demanded it.
 *
 * The hook is DECLARED here and defined nowhere in nano-ros: no default is
 * compiled, because a strong definition in an archive never replaces a weak
 * default that is already linked (RFC-0141 D2). The board supplies it as an
 * object of the image, or in an archive linked after nano-ros. */
#include <stdbool.h>
#include <stdint.h>

#include <nros/clock_epoch.h>

/* true: *out_ns is ns since the Unix epoch. false: not synced. */
bool nros_board_wall_clock_ns(uint64_t* out_ns);

bool nros_clock_board_try__set_by__knobs_clock_source(void);

/* Ask the board once and, when it answers, install the offset through the
 * ONE setter. Returns whether the offset was installed. */
bool nros_clock_board_try__set_by__knobs_clock_source(void) {
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
