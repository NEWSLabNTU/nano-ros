/// @file SafetyTalker.c
/// @brief Phase 269 W3 — C talker component for the E2E-safety workspace.
///
/// Publishes a monotonic counter on /chatter (std_msgs/Int32) every 1 s.
/// When built with NANO_ROS_SAFETY_E2E=ON (lowered from
/// `[system].features = ["safety"]` via NanoRosCapabilities.cmake), the
/// zenoh backend automatically attaches a CRC-32 + sequence number on every
/// publish — no code change required here. The paired C safe_listener
/// (`c_safety_listener_pkg`) validates this CRC via
/// `nros_cpp_subscription_register_validated`.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>
#include <nros/nros_cpp_ffi.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t counter;
} safety_talker_t;

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_tick(void* ctx) {
    safety_talker_t* self = (safety_talker_t*)ctx;
    /* std_msgs/Int32 CDR: 4-byte encapsulation header (CDR_LE) + int32 data. */
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)self->counter);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("[TALKER] Published: %d\n", self->counter);
        fflush(stdout);
    }
    self->counter++;
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor,
                                   safety_talker_t* self) {
    (void)executor;
    self->counter = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(safety_talker_t, talker_configure)
