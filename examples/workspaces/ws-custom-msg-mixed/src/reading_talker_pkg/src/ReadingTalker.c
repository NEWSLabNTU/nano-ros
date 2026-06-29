/// @file ReadingTalker.c
/// @brief ws-custom-msg-c — C talker for the IN-WORKSPACE custom message
///        `custom_msgs/Reading` (phase-263 B6, C projection of ws-custom-msg-rust).
///
/// `custom_msgs` is a real ROS 2 interface package (`package.xml` +
/// `msg/Reading.msg`) that lives inside this workspace; its schema is YOURS,
/// declared in-tree. As with every nano-ros C component (RFC-0043 / phase-257),
/// the raw publisher carries the type NAME as a string and the payload as
/// hand-encoded CDR bytes — identical to how the committed `c_talker_pkg`
/// publishes `std_msgs/Int32`, just with a workspace-local type. No generated C
/// bindings are consumed; the differentiator is that the message schema is the
/// workspace's own.
///
/// `talker_configure` creates a raw publisher on `/reading` + a 1 Hz timer that
/// publishes a CDR-encoded `Reading` whose `sequence` ramps every tick.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} reading_talker_t;

/* CDR encode of custom_msgs/Reading:
 *   [0..4)   encapsulation header (CDR_LE)
 *   [4..12)  float64 temperature (8-aligned: stream pos 0)
 *   [12..20) float64 humidity    (8-aligned: stream pos 8)
 *   [20..24) int32   sequence     (4-aligned: stream pos 16)
 * Total 24 payload bytes; the host is little-endian (x86_64) so the native
 * double/int byte order already matches CDR_LE — a plain memcpy suffices. */
static void on_tick(void* ctx) {
    reading_talker_t* self = (reading_talker_t*)ctx;
    double temperature = 20.0 + (double)self->count * 0.5;
    double humidity = 50.0;
    int32_t sequence = self->count;

    uint8_t buf[24];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    memcpy(buf + 4, &temperature, 8);
    memcpy(buf + 12, &humidity, 8);
    memcpy(buf + 20, &sequence, 4);

    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("[reading_talker] sent seq=%d temp=%.1f\n", (int)sequence, temperature);
    }
    self->count++;
}

static nros_ret_t reading_talker_configure(const nros_cpp_node_t* node, void* executor,
                                           reading_talker_t* self) {
    /* Line-buffer stdout so each `sent seq=` flushes immediately when piped (the
     * test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/reading", "custom_msgs::msg::dds_::Reading_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(reading_talker_t, reading_talker_configure)
