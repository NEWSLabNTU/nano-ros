/// @file talker.c
/// @brief C talker component: publishes `std_msgs/String` "Hello World: N"
///        on /chatter from a 1 Hz timer.
///
/// An ordinary typed C component, the same code as an RTOS board's. It names
/// no board and has no `main`: `RCLC_C_COMPONENT` emits the C-ABI factory and
/// configure hooks, and the entry the build generates for the bringup's image
/// calls them on the image's executor. On the bare-metal MPS2-AN385 that entry
/// is reached from the board's C start, after the clock, the LAN9118 and the
/// network are up (phase-553 W4).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rcl/rcl.h>
#include <rclc/executor.h>
#include <rclc/rclc.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    std_msgs__msg__String msg;
    // The bytes behind `msg.data`: the component owns them, so nothing allocates.
    std_msgs__msg__String__Storage msg_storage;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    self->count++;
    snprintf(self->msg.data.data, self->msg.data.capacity, "Hello World: %d", (int)self->count);
    self->msg.data.size = strlen(self->msg.data.data);
    if (rcl_publish(&self->pub, &self->msg, NULL) == RCL_RET_OK) {
        printf("Publishing: '%s'\n", self->msg.data.data);
    }
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    std_msgs__msg__String__init_in(&self->msg, &self->msg_storage);
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter");
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

RCLC_C_COMPONENT(talker_t, talker_configure)
