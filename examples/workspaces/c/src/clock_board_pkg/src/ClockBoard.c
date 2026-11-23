/// @file ClockBoard.c
/// @brief A stub board's wall clock (RFC-0141 D2), as a C component.
///
/// Only the `linux_talker_clock_board` image launches this component, and that image states
/// `[knobs.clock] source = "board"`, so the port asks the hook below on its first wall-clock
/// read. The hook sits in this TU, so no other image links it. A 1 s timer prints what the wall
/// clock then reads: `clock-board: wall_ns=<ns> synced=<0|1> source=<id> steps=<n>`, the wall
/// clock through `rcl_clock_get_now(RCL_SYSTEM_TIME)`, then `rclc_wall_clock_state()`.

#include <inttypes.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

/// The instant the stub board answers: 2023-11-14T22:13:20Z.
#define CLOCK_BOARD_INSTANT_NS 1700000000000000000ULL

/// The board's wall-clock hook: writes the board's instant and reports it set.
bool nros_board_wall_clock_ns(uint64_t* out_ns) {
    if (out_ns == NULL) {
        return false;
    }
    *out_ns = CLOCK_BOARD_INSTANT_NS;
    return true;
}

typedef struct {
    rcl_timer_t timer;
    rcl_clock_t clock;
} clock_board_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    clock_board_t* self = RCLC_CONTAINER_OF(timer, clock_board_t, timer);
    (void)last_call_time;
    int64_t wall_ns = 0;
    (void)rcl_clock_get_now(&self->clock, &wall_ns);
    const nros_wall_clock_state_t st = rclc_wall_clock_state();
    printf("clock-board: wall_ns=%" PRId64 " synced=%d source=%u steps=%" PRIu32 "\n", wall_ns,
           st.synced ? 1 : 0, (unsigned)st.source, st.steps);
}

static rcl_ret_t clock_board_configure(rcl_node_t* node, rclc_executor_t* executor,
                                       clock_board_t* self) {
    (void)node;
#ifdef _IONBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IONBF, 0);
#endif
    self->clock = rclc_clock_get_zero_initialized();
    rcl_ret_t rc = rcl_clock_init(RCL_SYSTEM_TIME, &self->clock, NULL);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(clock_board_t, clock_board_configure)
