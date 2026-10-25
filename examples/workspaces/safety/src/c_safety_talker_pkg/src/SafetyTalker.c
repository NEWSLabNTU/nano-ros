/// @file SafetyTalker.c
/// @brief C talker component for the E2E-safety workspace.
///
/// Publishes a monotonic counter on /chatter (std_msgs/Int32) every 1 s using
/// the generated C serializer (std_msgs__msg__Int32__serialize).
///
/// Nothing here declares or implements message integrity. The per-sample CRC
/// that the paired listener (`c_safety_listener_pkg`) validates is attached by
/// the runtime because the bringup declares `[system].features = ["safety"]` —
/// see `demo_bringup/system.toml`, which is where that declaration and its
/// consequences are documented.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t counter;
} safety_talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    safety_talker_t* self = RCLC_CONTAINER_OF(timer, safety_talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg;
    memset(&msg, 0, sizeof(msg));
    msg.data = self->counter;
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("[TALKER] Published: %d\n", self->counter);
        fflush(stdout);
    }
    self->counter++;
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor,
                                  safety_talker_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->counter = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t trc = rclc_timer_init_default(&self->timer, NULL, 1000ULL * 1000000ULL, on_tick);
    if (trc != RCL_RET_OK) {
        return trc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(safety_talker_t, talker_configure)
