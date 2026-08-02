/// @file QosListener.c
/// @brief ws-qos-c — C listener that subscribes `std_msgs/Int32` on /chatter with
///        a QoS profile that MATCHES the talker's publisher (phase-263 B4, C
///        projection of ws-qos-rust).
///
/// `qos_listener_configure` builds the SAME non-default `nros_cpp_qos_t`
/// (RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(10)) the talker declares and passes it
/// by value to `nros_cpp_subscription_register` — instead of `nros_c_qos_default()`.
/// QoS is a per-entity contract set in code; matching the profile is what lets the
/// QoS-tagged endpoints connect. The callback decodes the CDR Int32 and prints
/// `Received: N` so an external test can watch the QoS-matched delivery path
/// end-to-end across processes.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <nros/component.h>

typedef struct {
    int recv;
} qos_listener_t;

/// Byte-identical to the talker's `qos_profile()` — both endpoints must declare
/// the same RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(10) contract to connect.
static nros_cpp_qos_t qos_profile(void) {
    nros_cpp_qos_t q = nros_c_qos_default();
    q.reliability = NROS_C_QOS_RELIABLE;
    q.durability = NROS_C_QOS_TRANSIENT_LOCAL;
    q.history = NROS_C_QOS_KEEP_LAST;
    q.depth = 10;
    return q;
}

static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    qos_listener_t* self = (qos_listener_t*)ctx;
    std_msgs_msg_int32 msg;
    if (std_msgs_msg_int32_deserialize(&msg, data, len) != 0) {
        return;
    }
    printf("Received: %d\n", (int)msg.data);
    self->recv++;
}

static nros_ret_t qos_listener_configure(const nros_cpp_node_t* node, void* executor,
                                         qos_listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    /* Line-buffer stdout so each `Received:` flushes immediately when piped (an
     * external test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->recv = 0;
    size_t handle;
    int32_t rc = nros_cpp_subscription_register(
        node, "/chatter", std_msgs_msg_int32_get_type_name(), "", qos_profile(), on_raw, self,
        /*sched_context=*/0, &handle,
        /*callback_group=*/NULL);
    if (rc == 0) {
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(qos_listener_t, qos_listener_configure)
