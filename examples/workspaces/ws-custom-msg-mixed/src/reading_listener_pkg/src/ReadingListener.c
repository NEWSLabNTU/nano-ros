/// @file ReadingListener.c
/// @brief ws-custom-msg-c — C listener for the IN-WORKSPACE custom message
///        `custom_msgs/Reading` (phase-263 B6, C projection of ws-custom-msg-rust).
///
/// `listener_configure` binds `on_raw` as a raw zero-copy subscription on
/// `/reading`, carrying the workspace-local type name as a string (RFC-0043 /
/// phase-257). The callback decodes the CDR-encoded `Reading` and prints the
/// `sequence` + `temperature` fields so an external test can watch the custom
/// message flow end-to-end across processes.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

typedef struct {
    int recv;
} reading_listener_t;

/* CDR-encoded custom_msgs/Reading (see ReadingTalker.c for the layout):
 *   [4..12)  float64 temperature
 *   [12..20) float64 humidity
 *   [20..24) int32   sequence
 * Host is little-endian (x86_64), so memcpy back into native types matches. */
static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    reading_listener_t* self = (reading_listener_t*)ctx;
    if (len >= 24) {
        double temperature;
        int32_t sequence;
        memcpy(&temperature, data + 4, 8);
        memcpy(&sequence, data + 20, 4);
        printf("reading seq=%d temp=%.1f\n", (int)sequence, temperature);
        self->recv++;
    }
}

static nros_ret_t reading_listener_configure(const nros_cpp_node_t* node, void* executor,
                                             reading_listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    /* Line-buffer stdout so each `reading seq=` flushes immediately when piped
     * (an external test reads the output live; glibc block-buffers a pipe). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->recv = 0;
    size_t handle;
    int32_t rc = nros_cpp_subscription_register(node, "/reading", "custom_msgs::msg::dds_::Reading_",
                                                "", nros_c_qos_default(), on_raw, self,
                                                /*sched_context=*/0, &handle);
    if (rc == 0) {
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(reading_listener_t, reading_listener_configure)
