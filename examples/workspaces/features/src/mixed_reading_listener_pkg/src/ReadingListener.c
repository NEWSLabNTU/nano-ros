/// @file ReadingListener.c
/// @brief ws-custom-msg-c — C listener for the IN-WORKSPACE custom message
///        `custom_msgs/Reading` (phase-263 B6, C projection of ws-custom-msg-rust).
///
/// `listener_configure` subscribes to `/reading` with the C API and binds `on_msg`;
/// the executor decodes each sample with the GENERATED
/// `custom_msgs` typesupport (phase-293 / issue #212 — struct, `_deserialize`,
/// and type name all come from the bindings generated from `Reading.msg`) and
/// prints the `sequence` + `temperature` fields so an external test can watch
/// the custom message flow end-to-end across processes.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <custom_msgs/custom_msgs.h>
#include <nros/component.h>

typedef struct {
    rcl_subscription_t sub;
    custom_msgs__msg__Reading msg;
    int recv;
} reading_listener_t;

/* issue 0737 — a sample that fails to decode never reaches this callback: the
 * executor drops it, counts it and LOGS it (rate-limited), so a rejected
 * sample is never indistinguishable from one that never arrived. */
static void on_msg(const void* msgin, void* ctx) {
    reading_listener_t* self = (reading_listener_t*)ctx;
    const custom_msgs__msg__Reading* msg = (const custom_msgs__msg__Reading*)msgin;
    printf("reading seq=%d temp=%.1f\n", (int)msg->sequence, msg->temperature);
    self->recv++;
}

static rcl_ret_t reading_listener_configure(rcl_node_t* node, rclc_executor_t* executor,
                                            reading_listener_t* self) {
    /* Line-buffer stdout so each `reading seq=` flushes immediately when piped
     * (an external test reads the output live; glibc block-buffers a pipe). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->recv = 0;
    self->sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(custom_msgs, msg, Reading), "/reading");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg, on_msg, self,
                                                     ON_NEW_DATA);
    if (rc == RCL_RET_OK) {
        printf("Waiting for messages\n");
    }
    return rc;
}

RCLC_C_COMPONENT(reading_listener_t, reading_listener_configure)
