/// @file Talker.c
/// @brief Phase 269 W1 — parameterised C component.
///
/// Reads `publish_period_ms` from the executor's parameter store via
/// `nros_cpp_get_param_integer(executor, "publish_period_ms", &val)` on each tick
/// and publishes that value on /chatter (std_msgs/Int32). The launch `<param>` baked
/// initial is 250; a `ros2 param set publish_period_ms N` changes the published value
/// live, proving the in-callback live read path for C components.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>
#include <nros/nros_cpp_ffi.h> /* nros_cpp_get_param_integer, NROS_CPP_RET_OK */

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    void* executor; /* CppContext* — saved at configure, used in on_tick */
} talker_t;

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_tick(void* ctx) {
    talker_t* self = (talker_t*)ctx;
    /* Phase 269 W1 — live param read: re-read publish_period_ms from the executor's
     * volatile store each tick. Boots at the launch-baked initial (250); a
     * `ros2 param set publish_period_ms N` changes what we publish here. */
    int64_t live = -1;
    nros_cpp_get_param_integer(self->executor, "publish_period_ms", &live);

    /* std_msgs/Int32 CDR: 4-byte encapsulation header (CDR_LE) + int32 data. */
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)(int32_t)live);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("Published: %lld\n", (long long)live);
    }
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
    self->executor = executor;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(talker_t, talker_configure)
