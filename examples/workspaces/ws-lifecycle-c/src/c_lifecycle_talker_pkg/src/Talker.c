/// @file Talker.c
/// @brief Managed-node C component.
///
/// Publishes a monotonic counter on /chatter (std_msgs/Int32) every 1 s using
/// the generated C serializer (std_msgs_msg_int32_serialize).
/// The lifecycle state machine (register + Configure→Activate) is handled by the
/// generated entry's `__nros_entry_setup` via `nros_cpp_lifecycle_autostart` — this
/// component has no lifecycle callbacks of its own. The e2e test checks that
/// `ros2 lifecycle get /talker` returns `active` at boot.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/nros_cpp_ffi.h>
#include <nros/component.h>

#include "std_msgs.h"

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t counter;
} talker_t;

static void on_tick(void* ctx) {
    talker_t* self = (talker_t*)ctx;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);
    msg.data = self->counter;
    uint8_t buf[16];
    size_t len = 0;
    if (std_msgs_msg_int32_serialize(&msg, buf, sizeof(buf), &len) == 0 &&
        nros_cpp_publish_raw(self->pub, buf, len) == 0) {
        printf("Published: %d\n", self->counter);
    }
    self->counter++;
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->counter = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", std_msgs_msg_int32_get_type_name(),
                                           std_msgs_msg_int32_get_type_hash(), nros_c_qos_default(),
                                           self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(talker_t, talker_configure)
