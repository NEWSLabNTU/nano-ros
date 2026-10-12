/// @file Telem.c
/// @brief ws-realtime-c — low-tier telemetry node.
///
/// Publishes a monotonic counter on /telem every 100 ms (std_msgs/Int32,
/// generated C serializer). The cmake CALLBACK_GROUPS declares the "telem"
/// group ID; system.toml [[node_overrides]] maps it to the [tiers.low] tier.
/// Running at 1/10 the cadence of ctrl_pkg, the e2e test asserts ctrl publishes
/// at least 3× as many messages as telem in the same window.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} telem_t;

static void on_tick(rcl_timer_t* timer, void* ctx) {
    (void)timer;
    telem_t* self = (telem_t*)ctx;
    std_msgs__msg__Int32 msg = {.data = self->count};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("[telem] tick=%d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t telem_configure(rcl_node_t* node, rclc_executor_t* executor, telem_t* self) {
    /* Line-buffer stdout so each tick flushes immediately when piped. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/telem");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_with_context(&self->timer, 100ULL * 1000000ULL, on_tick, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(telem_t, telem_configure)
