/// @file QosTalker.c
/// @brief ws-qos-c — C talker that publishes `std_msgs/Int32` on /chatter with an
///        EXPLICIT, NON-DEFAULT QoS profile (C projection of ws-qos-rust).
///
/// The nano-ros QoS differentiator, in C: instead of `nros_c_qos_default()`
/// (reliable + VOLATILE + keep-last-10) the committed `c_talker_pkg` passes, this
/// builds a `nros_cpp_qos_t` with reliability=RELIABLE, durability=TRANSIENT_LOCAL,
/// history=KEEP_LAST, depth=10 and passes it BY VALUE to
/// `nros_cpp_publisher_create`. QoS is a per-entity contract set in code (no
/// launch `qos_overrides`); the matching `qos_listener_pkg` declares the SAME
/// profile via `qos_profile()` so the two endpoints connect.
///
/// `qos_talker_configure` creates the QoS-tagged publisher on /chatter + a
/// 1 Hz timer that publishes an Int32 counter via the generated C serializer
/// (std_msgs_msg_int32_serialize) — identical wire shape to the committed
/// pure-C talker, just with a non-default QoS contract.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include "std_msgs.h"

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} qos_talker_t;

/// The shared QoS contract both endpoints declare: RELIABLE delivery,
/// TRANSIENT_LOCAL durability, KEEP_LAST(10) history depth. The listener builds
/// the byte-identical profile so the QoS-matched endpoints connect.
static nros_cpp_qos_t qos_profile(void) {
    nros_cpp_qos_t q = nros_c_qos_default();
    q.reliability = NROS_C_QOS_RELIABLE;
    q.durability = NROS_C_QOS_TRANSIENT_LOCAL;
    q.history = NROS_C_QOS_KEEP_LAST;
    q.depth = 10;
    return q;
}

static void on_tick(void* ctx) {
    qos_talker_t* self = (qos_talker_t*)ctx;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);
    msg.data = self->count;
    uint8_t buf[16];
    size_t len = 0;
    if (std_msgs_msg_int32_serialize(&msg, buf, sizeof(buf), &len) == 0 &&
        nros_cpp_publish_raw(self->pub, buf, len) == 0) {
        printf("Published: %d\n", (int)self->count);
    }
    self->count++;
}

static nros_ret_t qos_talker_configure(const nros_cpp_node_t* node, void* executor,
                                       qos_talker_t* self) {
    /* Line-buffer stdout so each `Published:` flushes immediately when piped (the
     * test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    int32_t rc =
        nros_cpp_publisher_create(node, "/chatter", std_msgs_msg_int32_get_type_name(),
                                  std_msgs_msg_int32_get_type_hash(), qos_profile(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(qos_talker_t, qos_talker_configure)
