/*
 * The echo half of the RPMsg round-trip bench: every /ping goes straight
 * back on /pong, from the subscription callback, so the round trip the ping
 * side times holds one dispatch on each core and nothing else of this one.
 */
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_subscription_t sub;
    std_msgs__msg__UInt32 in;
    uint32_t echoed;
} pong_t;

static void on_ping(const void* msgin, void* ctx) {
    pong_t* self = (pong_t*)ctx;
    if (rcl_publish(&self->pub, (const std_msgs__msg__UInt32*)msgin, NULL) == RCL_RET_OK) {
        self->echoed++;
    }
}

static rcl_ret_t pong_configure(rcl_node_t* node, rclc_executor_t* executor, pong_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->echoed = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, UInt32), "/pong");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->sub = rcl_get_zero_initialized_subscription();
    rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, UInt32), "/ping");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    printf("pong: echoing /ping on /pong\n");
    return rclc_executor_add_subscription_with_context(executor, &self->sub, &self->in, on_ping,
                                                       self, ON_NEW_DATA);
}

RCLC_C_COMPONENT(pong_t, pong_configure)
