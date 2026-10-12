/// @file Talker.c
/// @brief Parameterised C component.
///
/// Reads `publish_period_ms` from the node's parameters via rclc's
/// `rclc_parameter_get_int(&server, "publish_period_ms", &val)` on each tick
/// and publishes that value on /chatter (std_msgs/Int32, generated C serializer).
/// The launch `<param>` baked initial is 250; a `ros2 param set publish_period_ms N`
/// changes the published value live, proving the in-callback live read path for
/// C components.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>
#include <rclc_parameter/rclc_parameter.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    /* A handle over the parameters the entry already serves for this node:
     * the store is the executor's, and the services were registered by the
     * host, so this is never added to the executor. */
    rclc_parameter_server_t params;
} talker_t;

static void on_tick(rcl_timer_t* timer, void* ctx) {
    (void)timer;
    talker_t* self = (talker_t*)ctx;
    /* Live param read: re-read publish_period_ms from the node's volatile
     * store each tick. Boots at the launch-baked initial (250); a
     * `ros2 param set publish_period_ms N` changes what we publish here. */
    int64_t live = -1;
    (void)rclc_parameter_get_int(&self->params, "publish_period_ms", &live);

    std_msgs__msg__Int32 msg;
    memset(&msg, 0, sizeof(msg));
    msg.data = (int32_t)live;
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("Published: %lld\n", (long long)live);
    }
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    rcl_ret_t prc = rclc_parameter_server_init_default(&self->params, node);
    if (prc != RCL_RET_OK) {
        return prc;
    }
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t trc = rclc_timer_init_with_context(&self->timer, 500ULL * 1000000ULL, on_tick, self);
    if (trc != RCL_RET_OK) {
        return trc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
