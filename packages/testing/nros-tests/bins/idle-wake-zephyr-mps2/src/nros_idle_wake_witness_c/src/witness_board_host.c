/// @file witness_board_host.c
/// @brief The idle-wake witness's board seam where there is no board.
///
/// What the host metadata probe compiles in place of witness_board_zephyr.c:
/// the probe reads what the node DECLARES (its subscription and timer), which
/// does not depend on the board, so this measures nothing. A wall clock for
/// the timestamps, an empty switch-in count, and no receive interrupt.

#include <time.h>

#include "witness_board.h"

int64_t witness_board_now_us(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (int64_t)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;
}

void witness_board_open_window(void) {}

uint32_t witness_board_close_window(void) {
    return 0;
}

bool witness_board_arrival_us(uint32_t* us) {
    (void)us;
    return false;
}
