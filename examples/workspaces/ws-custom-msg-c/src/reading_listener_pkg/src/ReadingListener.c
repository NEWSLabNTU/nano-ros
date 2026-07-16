/// @file ReadingListener.c
/// @brief ws-custom-msg-c — C listener for the IN-WORKSPACE custom message
///        `custom_msgs/Reading` (phase-263 B6, C projection of ws-custom-msg-rust).
///
/// `listener_configure` binds `on_raw` as a raw zero-copy subscription on
/// `/reading`. The callback decodes the payload with the GENERATED
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
    int recv;
} reading_listener_t;

static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    reading_listener_t* self = (reading_listener_t*)ctx;
    custom_msgs_msg_reading msg;
    if (custom_msgs_msg_reading_deserialize(&msg, data, len) == 0) {
        printf("reading seq=%d temp=%.1f\n", (int)msg.sequence, msg.temperature);
        self->recv++;
    }
}

static nros_ret_t reading_listener_configure(const nros_cpp_node_t* node, void* executor,
                                             reading_listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    /* Line-buffer stdout so each `reading seq=` flushes immediately when piped
     * (an external test reads the output live; glibc block-buffers a pipe). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->recv = 0;
    size_t handle;
    int32_t rc =
        nros_cpp_subscription_register(node, "/reading", custom_msgs_msg_reading_get_type_name(),
                                       "", nros_c_qos_default(), on_raw, self,
                                       /*sched_context=*/0, &handle,
                                       /*callback_group=*/NULL);
    if (rc == 0) {
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(reading_listener_t, reading_listener_configure)
