/// @file QosListener.c
/// @brief ws-qos-c — C listener that subscribes `std_msgs/Int32` on /chatter with
///        a QoS profile that MATCHES the talker's publisher (phase-263 B4, C
///        projection of ws-qos-rust).
///
/// `qos_listener_configure` builds the SAME non-default `rclc_qos_t`
/// (RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(1)) the talker declares and passes it
/// to `rclc_subscription_init_with_qos` — instead of `RCLC_QOS_DEFAULT`.
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
    rcl_subscription_t sub;
    std_msgs__msg__Int32 msg;
    int recv;
} qos_listener_t;

/// Byte-identical to the talker's `qos_profile()` — both endpoints must declare
/// the same RELIABLE + TRANSIENT_LOCAL + KEEP_LAST(1) contract to connect.
static rclc_qos_t qos_profile(void) {
    rclc_qos_t q = RCLC_QOS_DEFAULT;
    q.reliability = RCLC_QOS_RELIABILITY_RELIABLE;
    q.durability = RCLC_QOS_DURABILITY_TRANSIENT_LOCAL;
    q.history = RCLC_QOS_HISTORY_KEEP_LAST;
    q.depth = 1;
    return q;
}

/* issue 0737 — a sample that fails to decode never reaches this callback: the
 * executor drops it, counts it and LOGS it (rate-limited), so a rejected
 * sample is never indistinguishable from one that never arrived. */
static void on_msg(const void* msgin, void* ctx) {
    qos_listener_t* self = (qos_listener_t*)ctx;
    const std_msgs__msg__Int32* msg = (const std_msgs__msg__Int32*)msgin;
    printf("Received: %d\n", (int)msg->data);
    self->recv++;
}

static rcl_ret_t qos_listener_configure(rcl_node_t* node, rclc_executor_t* executor,
                                        qos_listener_t* self) {
    /* Line-buffer stdout so each `Received:` flushes immediately when piped (an
     * external test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->recv = 0;
    const rclc_qos_t qos = qos_profile();
    self->sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_with_qos(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter", &qos);
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

RCLC_C_COMPONENT(qos_listener_t, qos_listener_configure)
