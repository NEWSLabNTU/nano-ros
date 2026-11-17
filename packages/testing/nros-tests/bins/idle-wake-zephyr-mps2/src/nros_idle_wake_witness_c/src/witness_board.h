/// @file witness_board.h
/// @brief What the idle-wake witness measures on the board, behind one seam.
///
/// Witness.c is the node: a subscription, a timer and their callbacks. The
/// measurement it reports (switch-ins of the executor's thread, the cycle
/// counter at the last LAN9118 receive interrupt) is Zephyr's and the board's,
/// so it lives in witness_board_zephyr.c, which the component builds only
/// inside a Zephyr build. Everywhere else, the host metadata probe included,
/// witness_board_host.c answers: the probe compiles the node to read what it
/// declares, and the node does not need the board to say that.

#ifndef NROS_IDLE_WAKE_WITNESS_BOARD_H
#define NROS_IDLE_WAKE_WITNESS_BOARD_H

#include <stdbool.h>
#include <stdint.h>

/// Microseconds since boot.
int64_t witness_board_now_us(void);

/// Start counting switch-ins of the calling thread (the executor's).
void witness_board_open_window(void);

/// Stop counting, and return the switch-ins seen since the window opened.
uint32_t witness_board_close_window(void);

/// The wakes in the last window: switch-ins whose preceding switch-out left
/// the thread blocked (pending, suspended or sleeping). A subset of the
/// switch-ins, which also count resumptions after a preemption.
uint32_t witness_board_wakes(void);

/// Microseconds from the last receive interrupt to now, or false when no
/// interrupt has been seen.
bool witness_board_arrival_us(uint32_t* us);

#endif
