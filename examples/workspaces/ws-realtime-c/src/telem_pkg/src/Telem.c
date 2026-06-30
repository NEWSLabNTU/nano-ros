/// @file Telem.c
/// @brief ws-realtime-c — Phase 269 W4 low-tier telemetry node.
///
/// Publishes a monotonic counter on /telem every 100 ms (std_msgs/Int32,
/// hand-encoded CDR). The cmake CALLBACK_GROUPS declares the "telem" group ID;
/// system.toml [[node_overrides]] maps it to the [tiers.low] tier.
/// Running at 1/10 the cadence of ctrl_pkg, the e2e test asserts ctrl publishes
/// at least 3× as many messages as telem in the same window.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} telem_t;

/* CDR encode of std_msgs/Int32:
 *   [0..4)  encapsulation header (CDR_LE)
 *   [4..8)  int32 data
 * Total 8 bytes; the host is little-endian — plain memcpy suffices. */
static void on_tick(void *ctx) {
    telem_t *self = (telem_t *)ctx;
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    memcpy(buf + 4, &self->count, 4);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("[telem] tick=%d\n", (int)self->count);
    }
    self->count++;
}

static nros_ret_t telem_configure(const nros_cpp_node_t *node, void *executor, telem_t *self) {
    /* Line-buffer stdout so each tick flushes immediately when piped. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/telem", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/100, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(telem_t, telem_configure)
