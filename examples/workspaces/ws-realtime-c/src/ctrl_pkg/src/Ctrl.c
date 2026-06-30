/// @file Ctrl.c
/// @brief ws-realtime-c — Phase 269 W4 high-tier control node.
///
/// Publishes a monotonic counter on /ctrl every 10 ms (std_msgs/Int32,
/// hand-encoded CDR). The cmake CALLBACK_GROUPS declares the "ctrl" group ID;
/// system.toml [[node_overrides]] maps it to the [tiers.high] tier.
/// The nros codegen emits nros_cpp_create_sched_context + nros_cpp_node_create_ex
/// to bind this node's timer to the high-priority sched context (RFC-0015 §4.2).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} ctrl_t;

/* CDR encode of std_msgs/Int32:
 *   [0..4)  encapsulation header (CDR_LE)
 *   [4..8)  int32 data
 * Total 8 bytes; the host is little-endian — plain memcpy suffices. */
static void on_tick(void *ctx) {
    ctrl_t *self = (ctrl_t *)ctx;
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    memcpy(buf + 4, &self->count, 4);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("[ctrl] tick=%d\n", (int)self->count);
    }
    self->count++;
}

static nros_ret_t ctrl_configure(const nros_cpp_node_t *node, void *executor, ctrl_t *self) {
    /* Line-buffer stdout so each tick flushes immediately when piped. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/ctrl", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/10, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(ctrl_t, ctrl_configure)
