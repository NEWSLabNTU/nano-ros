/// @file Listener.c
/// @brief pure-C workspace — C listener, typed component (RFC-0043 / phase-257).
///
/// `listener_configure` binds `on_raw` (by identity, fn-ptr + self ctx) as a raw
/// zero-copy subscription on `/chatter`. `NROS_C_COMPONENT` emits the C-ABI
/// factory/configure the typed C Entry calls. No declarative descriptor, no
/// interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <nros/component.h>

typedef struct {
    int recv;
} listener_t;

static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    std_msgs_msg_int32 msg;
    /* issue 0737 — say why, never just `return`. A silent drop here is
     * indistinguishable from "the message never arrived", and that ambiguity
     * cost two hosts a full investigation each: the sample was matched, stored
     * and taken (Cyclone's own trace said `take: returning 1`) while the only
     * observable was an absence of output. Whatever is wrong, the reader is
     * entitled to know a message reached this callback and was rejected. */
    if (std_msgs_msg_int32_deserialize(&msg, data, len) != 0) {
        fprintf(stderr, "listener: DROPPED a sample — deserialize failed, %zu byte(s)\n", len);
        return;
    }
    printf("Received: %d\n", (int)msg.data);
    self->recv++;
}

static nros_ret_t listener_configure(const nros_cpp_node_t* node, void* executor,
                                     listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    self->recv = 0;
    size_t handle;
    int32_t rc =
        nros_cpp_subscription_register(node, "/chatter", std_msgs_msg_int32_get_type_name(), "",
                                       nros_c_qos_default(), on_raw, self, &handle,
                                       /*options=*/NULL); /* phase-402: NULL = defaults */
    if (rc == 0) {
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(listener_t, listener_configure)
