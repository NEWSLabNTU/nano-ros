/// @file ReadingTalker.c
/// @brief ws-custom-msg-c — C talker for the IN-WORKSPACE custom message
///        `custom_msgs/Reading` (phase-263 B6, C projection of ws-custom-msg-rust).
///
/// `custom_msgs` is a real ROS 2 interface package (`package.xml` +
/// `msg/Reading.msg`) that lives inside this workspace; its schema is YOURS,
/// declared in-tree. The publisher uses the GENERATED C typesupport
/// (phase-293 / issue #212): struct, `_serialize`, and type name all come
/// from the `custom_msgs` bindings the build generates from `Reading.msg` —
/// exactly how every committed C example consumes `std_msgs`. Add a field to
/// `Reading.msg` and only the code that USES the new field changes.
///
/// `talker_configure` creates a publisher on `/reading` + a 1 Hz timer that
/// publishes a `Reading` whose `sequence` ramps every tick.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <custom_msgs/custom_msgs.h>
#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} reading_talker_t;

static void on_tick(void* ctx) {
    reading_talker_t* self = (reading_talker_t*)ctx;

    custom_msgs_msg_reading msg;
    custom_msgs_msg_reading_init(&msg);
    msg.temperature = 20.0 + (double)self->count * 0.5;
    msg.humidity = 50.0;
    msg.sequence = self->count;

    uint8_t buf[64];
    size_t n = 0;
    if (custom_msgs_msg_reading_serialize(&msg, buf, sizeof(buf), &n) == 0 &&
        nros_cpp_publish_raw(self->pub, buf, n) == 0) {
        printf("[reading_talker] sent seq=%d temp=%.1f\n", (int)msg.sequence, msg.temperature);
    }
    self->count++;
}

static nros_ret_t reading_talker_configure(const nros_cpp_node_t* node, void* executor,
                                           reading_talker_t* self) {
    /* Line-buffer stdout so each `sent seq=` flushes immediately when piped (the
     * test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    int32_t rc =
        nros_cpp_publisher_create(node, "/reading", custom_msgs_msg_reading_get_type_name(), "",
                                  nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(reading_talker_t, reading_talker_configure)
