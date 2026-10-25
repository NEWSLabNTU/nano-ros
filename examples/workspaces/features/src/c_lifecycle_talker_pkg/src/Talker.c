/// @file Talker.c
/// @brief Managed-node C component.
///
/// Publishes a monotonic counter on /chatter (std_msgs/Int32) every 1 s with the
/// C API (`rcl_publish`).
/// The lifecycle state machine (register + Configure→Activate) is handled by the
/// generated entry when the node is created — this component has no lifecycle
/// callbacks of its own. The e2e test checks that
/// `ros2 lifecycle get /talker` returns `active` at boot.

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
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg;
    memset(&msg, 0, sizeof(msg));
    msg.data = self->counter;
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("Published: %d\n", self->counter);
    }
    self->counter++;
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
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

RCLC_C_COMPONENT(talker_t, talker_configure)
