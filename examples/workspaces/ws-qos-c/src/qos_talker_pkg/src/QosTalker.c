/// @file QosTalker.c
/// @brief ws-qos-c — C talker that publishes `std_msgs/Int32` on /chatter with an
///        EXPLICIT, NON-DEFAULT QoS profile (phase-263 B4, C projection of
///        ws-qos-rust).
///
/// The nano-ros QoS differentiator, in C: instead of `nros_c_qos_default()`
/// (reliable + VOLATILE + keep-last-10) the committed `c_talker_pkg` passes, this
/// builds a `nros_cpp_qos_t` with reliability=RELIABLE, durability=TRANSIENT_LOCAL,
/// history=KEEP_LAST, depth=10 and passes it BY VALUE to
/// `nros_cpp_publisher_create`. QoS is a per-entity contract set in code (no
/// launch `qos_overrides`); the matching `qos_listener_pkg` declares the SAME
/// profile via `qos_profile()` so the two endpoints connect.
///
/// `qos_talker_configure` creates the QoS-tagged raw publisher on /chatter + a
/// 1 Hz timer that publishes a CDR-encoded Int32 counter — identical wire shape
/// to the committed pure-C talker, just with a non-default QoS contract.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

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

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_tick(void* ctx) {
    qos_talker_t* self = (qos_talker_t*)ctx;
    /* std_msgs/Int32 CDR: 4-byte encapsulation header (CDR_LE) + int32 data. */
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)self->count);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
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
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                           qos_profile(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(qos_talker_t, qos_talker_configure)
