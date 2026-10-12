/// @file Ctrl.c
/// @brief ws-realtime-c — high-tier control node.
///
/// Publishes a monotonic counter on /ctrl every 10 ms (std_msgs/Int32,
/// generated C serializer). The cmake CALLBACK_GROUPS declares the "ctrl"
/// group ID; system.toml [[node_overrides]] maps it to the [tiers.high] tier.
/// The generated entry creates this node on the high-priority sched context, so
/// every entity the component creates on it lands there (RFC-0015 §4.2).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} ctrl_t;

static void on_tick(rcl_timer_t* timer, void* ctx) {
    (void)timer;
    ctrl_t* self = (ctrl_t*)ctx;
    std_msgs__msg__Int32 msg;
    memset(&msg, 0, sizeof(msg));
    msg.data = self->count;
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("[ctrl] tick=%d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t ctrl_configure(rcl_node_t* node, rclc_executor_t* executor, ctrl_t* self) {
    /* Line-buffer stdout so each tick flushes immediately when piped. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/ctrl");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t trc = rclc_timer_init_with_context(&self->timer, 10ULL * 1000000ULL, on_tick, self);
    if (trc != RCL_RET_OK) {
        return trc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(ctrl_t, ctrl_configure)
