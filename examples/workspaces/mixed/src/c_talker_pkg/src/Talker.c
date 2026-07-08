/// @file Talker.c
/// @brief mixed workspace — C talker, typed component (RFC-0043).
///
/// `talker_configure` creates a publisher on `/chatter` + a 1 Hz timer that
/// publishes an Int32 counter via the generated C serializer
/// (std_msgs_msg_int32_serialize). `NROS_C_COMPONENT` emits the C-ABI
/// factory/configure the typed C++ Entry carrier calls; it runs on the real
/// executor (NativeBoard::run_components) — no hand-written boilerplate.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>
#include <nros/log.h> /* A5 — node logging via the nros-log facade */

#include "std_msgs.h"

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} talker_t;

static void on_tick(void* ctx) {
    talker_t* self = (talker_t*)ctx;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);
    msg.data = self->count;
    uint8_t buf[16];
    size_t len = 0;
    if (std_msgs_msg_int32_serialize(&msg, buf, sizeof(buf), &len) == 0 &&
        nros_cpp_publish_raw(self->pub, buf, len) == 0) {
        printf("[c_talker_pkg] sent: %d\n", (int)self->count);
    }
    /* A5 — log each tick via the nros-log facade. `nros_log_default_logger()` is the built-in
     * DEFAULT_LOGGER (level Info; NULL would DROP the record). The first emit lazy-installs the
     * default sink → the posix platform writer → "[INFO] nros: c_talker logging seq=N". */
    NROS_LOG_INFO(nros_log_default_logger(), "c_talker logging seq=%d", (int)self->count);
    self->count++;
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
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
