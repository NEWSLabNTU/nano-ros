/// @file Listener.c
/// @brief Zephyr C listener — typed component (RFC-0043 / phase-244.C2).
///
/// A stateful C component: `listener_configure` binds the `on_raw` callback (by
/// identity, fn-ptr + self ctx) as a raw zero-copy subscription on `/chatter`.
/// `NROS_C_COMPONENT` emits the C-ABI factory + configure the Zephyr typed Entry
/// carrier (`zephyr_entry_main_c_typed.cpp.in`) calls. No declarative descriptor,
/// no synthesizing interpreter, no callback name.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    int recv;
} listener_t;

static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    /* CDR-encoded std_msgs/Int32: 4-byte encapsulation header, then LE i32. */
    int32_t v = 0;
    if (len >= 8) {
        v = (int32_t)((uint32_t)data[4] | ((uint32_t)data[5] << 8) | ((uint32_t)data[6] << 16) |
                      ((uint32_t)data[7] << 24));
    }
    printf("Received: %d\n", (int)v);
    self->recv++;
}

static nros_ret_t listener_configure(const nros_cpp_node_t* node, void* executor,
                                     listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    size_t handle;
    int32_t rc = nros_cpp_subscription_register(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                                nros_c_qos_default(), on_raw, self,
                                                /*sched_context=*/0, &handle,
                                                /*callback_group=*/NULL);
    if (rc == 0) {
        /* Readiness marker the rtos_e2e harness greps before driving the talker. */
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(listener_t, listener_configure)
