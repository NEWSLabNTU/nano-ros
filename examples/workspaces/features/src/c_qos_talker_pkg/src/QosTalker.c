/// @file QosTalker.c
/// @brief ws-qos-c — C talker that publishes `std_msgs/Int32` on /chatter with an
///        EXPLICIT, NON-DEFAULT QoS profile (C projection of ws-qos-rust).
///
/// The nano-ros QoS differentiator, in C: instead of `RCLC_QOS_DEFAULT`
/// (reliable + VOLATILE + keep-last-10) the committed `c_talker_pkg` uses, this
/// builds an `rclc_qos_t` with reliability=RELIABLE, durability=TRANSIENT_LOCAL,
/// history=KEEP_LAST, depth=1 and passes it to `rclc_publisher_init_with_qos`. QoS is a per-entity
/// contract set in code (no launch `qos_overrides`); the matching `c_qos_listener_pkg` declares the
/// SAME profile via `qos_profile()` so the two endpoints connect.
///
/// `qos_talker_configure` creates the QoS-tagged publisher on /chatter + a
/// 1 Hz timer that publishes an Int32 counter via the generated C serializer
/// (std_msgs__msg__Int32__serialize) — identical wire shape to the committed
/// pure-C talker, just with a non-default QoS contract.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

#include "std_msgs.h"

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} qos_talker_t;

/// The shared QoS contract both endpoints declare: RELIABLE delivery,
/// TRANSIENT_LOCAL durability, KEEP_LAST(1) history depth. The listener builds
/// the byte-identical profile so the QoS-matched endpoints connect.
///
/// Depth 1 is what the zenoh backend SERVES for a transient-local publisher (it
/// retains one sample), and a stock `ros2 topic info -v` reads the served
/// profile — KEEP_LAST(10) was advertised as 1 (issue 1687).
static rclc_qos_t qos_profile(void) {
    rclc_qos_t q = RCLC_QOS_DEFAULT;
    q.reliability = RCLC_QOS_RELIABILITY_RELIABLE;
    q.durability = RCLC_QOS_DURABILITY_TRANSIENT_LOCAL;
    q.history = RCLC_QOS_HISTORY_KEEP_LAST;
    q.depth = 1;
    return q;
}

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    qos_talker_t* self = RCLC_CONTAINER_OF(timer, qos_talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg;
    memset(&msg, 0, sizeof(msg));
    msg.data = self->count;
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("Published: %d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t qos_talker_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      qos_talker_t* self) {
    /* Line-buffer stdout so each `Published:` flushes immediately when piped (the
     * test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    const rclc_qos_t qos = qos_profile();
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_with_qos(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter", &qos);
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

RCLC_C_COMPONENT(qos_talker_t, qos_talker_configure)
