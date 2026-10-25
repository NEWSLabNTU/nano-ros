/// @file ReadingTalker.c
/// @brief ws-custom-msg-c — C talker for the IN-WORKSPACE custom message
///        `custom_msgs/Reading` (phase-263 B6, C projection of ws-custom-msg-rust).
///
/// `custom_msgs` is a real ROS 2 interface package (`package.xml` +
/// `msg/Reading.msg`) that lives inside this workspace; its schema is YOURS,
/// declared in-tree. The publisher uses the GENERATED C typesupport
/// (phase-293 / issue #212): struct, `_serialize`, and type name all come
/// from the `custom_msgs` bindings the build generates from `Reading.msg` —
/// exactly how every committed C example consumes `std_msgs`. Add a field to
/// `Reading.msg` and only the code that USES the new field changes.
///
/// `talker_configure` creates a publisher on `/reading` + a 1 Hz timer that
/// publishes a `Reading` whose `sequence` ramps every tick.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <custom_msgs/custom_msgs.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} reading_talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    reading_talker_t* self = RCLC_CONTAINER_OF(timer, reading_talker_t, timer);
    (void)last_call_time;

    custom_msgs__msg__Reading msg;
    memset(&msg, 0, sizeof(msg));
    msg.temperature = 20.0 + (double)self->count * 0.5;
    msg.humidity = 50.0;
    msg.sequence = self->count;

    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("[reading_talker] sent seq=%d temp=%.1f\n", (int)msg.sequence, msg.temperature);
    }
    self->count++;
}

static rcl_ret_t reading_talker_configure(rcl_node_t* node, rclc_executor_t* executor,
                                          reading_talker_t* self) {
    /* Line-buffer stdout so each `sent seq=` flushes immediately when piped (the
     * test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(custom_msgs, msg, Reading), "/reading");
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

RCLC_C_COMPONENT(reading_talker_t, reading_talker_configure)
