// Issue 2405 fixture -- publishes a counter on /count (std_msgs/Int32) once a
// second. A BOUNDED type, so the image's message-bound sizes derive, and a
// configure that composes them without a seed is refused, not warned.

#include <stdint.h>
#include <string.h>

#include <rclc/rclc.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t since_last) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)since_last;
    std_msgs__msg__Int32 msg;
    memset(&msg, 0, sizeof(msg));
    msg.data = self->count++;
    (void)rcl_publish(&self->pub, &msg, NULL);
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/count");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 1000ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
