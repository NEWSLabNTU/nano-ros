/// @file Listener.c
/// @brief QEMU RISC-V ThreadX C listener — typed component (RFC-0043, phase-246).
///
/// `listener_configure` binds `on_raw` (by identity, fn-ptr + self ctx) as a raw
/// zero-copy subscription on `/chatter`. NROS_C_COMPONENT emits the C-ABI factory
/// + configure the typed carrier calls. No declarative descriptor, no interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include <std_msgs/msg/std_msgs_msg_string.h>

typedef struct {
    int recv;
    /* Decoded message lives in the component (not the callback stack):
     * String is ~256 B and RTOS dispatch stacks are small. */
    std_msgs_msg_string msg;
} listener_t;

static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    std_msgs_msg_string_init(&self->msg);
    if (std_msgs_msg_string_deserialize(&self->msg, data, len) == 0) {
        printf("I heard: [%s]\n", self->msg.data);
        self->recv++;
    }
}

static nros_ret_t listener_configure(const nros_cpp_node_t* node, void* executor,
                                     listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    size_t handle;
    int32_t rc =
        nros_cpp_subscription_register(node, "/chatter", std_msgs_msg_string_get_type_name(), "",
                                       nros_c_qos_default(), on_raw, self,
                                       /*sched_context=*/0, &handle,
                                       /*callback_group=*/NULL);
    if (rc == 0) {
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(listener_t, listener_configure)
