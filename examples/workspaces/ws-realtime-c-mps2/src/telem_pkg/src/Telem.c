/// @file Telem.c
/// @brief ws-realtime-c — low-tier telemetry node.
///
/// Publishes a monotonic counter on /telem every 100 ms (std_msgs/Int32,
/// generated C serializer). The cmake CALLBACK_GROUPS declares the "telem"
/// group ID; system.toml [[node_overrides]] maps it to the [tiers.low] tier.
/// Running at 1/10 the cadence of ctrl_pkg, the e2e test asserts ctrl publishes
/// at least 3× as many messages as telem in the same window.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include "std_msgs.h"

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} telem_t;

static void on_tick(void* ctx) {
    telem_t* self = (telem_t*)ctx;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);
    msg.data = self->count;
    uint8_t buf[16];
    size_t len = 0;
    if (std_msgs_msg_int32_serialize(&msg, buf, sizeof(buf), &len) == 0 &&
        nros_cpp_publish_raw(self->pub, buf, len) == 0) {
        printf("[telem] tick=%d\n", (int)self->count);
    }
    self->count++;
}

static nros_ret_t telem_configure(const nros_cpp_node_t* node, void* executor, telem_t* self) {
    /* Line-buffer stdout so each tick flushes immediately when piped. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/telem", std_msgs_msg_int32_get_type_name(),
                                           std_msgs_msg_int32_get_type_hash(), nros_c_qos_default(),
                                           self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/100, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(telem_t, telem_configure)
