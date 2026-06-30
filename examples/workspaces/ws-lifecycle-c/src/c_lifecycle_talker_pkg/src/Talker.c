/// @file Talker.c
/// @brief Phase 269 W2 — managed-node C component.
///
/// Publishes a monotonic counter on /chatter (std_msgs/Int32) every 1 s.
/// The lifecycle state machine (register + Configure→Activate) is handled by the
/// generated entry's `__nros_entry_setup` via `nros_cpp_lifecycle_autostart` — this
/// component has no lifecycle callbacks of its own. The e2e test checks that
/// `ros2 lifecycle get /talker` returns `active` at boot.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>
#include <nros/nros_cpp_ffi.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t counter;
} talker_t;

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_tick(void* ctx) {
    talker_t* self = (talker_t*)ctx;
    /* std_msgs/Int32 CDR: 4-byte encapsulation header (CDR_LE) + int32 data. */
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)self->counter);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("Published: %d\n", self->counter);
    }
    self->counter++;
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
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

NROS_C_COMPONENT(talker_t, talker_configure)
