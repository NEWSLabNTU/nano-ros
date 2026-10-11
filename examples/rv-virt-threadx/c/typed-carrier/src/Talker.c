/// @file Talker.c
/// @brief rv-virt-threadx — a C talker deployed through the ThreadX TYPED
/// CARRIER (`nros_components_register_node(... TYPED DEPLOY threadx)`), the
/// road `nano_ros_entry()` does not take (issue 1795). Same component as
/// `examples/templates/multi-package-workspace/src/pkg_c_talker`.
///
/// `talker_configure` creates a raw publisher on `/chatter` + a 1 Hz timer that
/// publishes a CDR-encoded Int32 counter. `RCLC_C_COMPONENT` emits the C-ABI
/// factory/configure the native typed Entry carrier (the shared entry pack)
/// calls; the carrier runs it on the real executor (NativeBoard::run_components) —
/// no hand-written `nros_app_main`, no `rclc_support_init`/`node_init`/
/// `executor_init`/spin-loop boilerplate (issue-0049 P1/P7).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <nros/component.h>

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
    size_t n = 0;
    if (std_msgs_msg_int32_serialize(&msg, buf, sizeof(buf), &n) == 0 &&
        nros_cpp_publish_raw(self->pub, buf, n) == 0) {
        printf("[typed_carrier] sent: %d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", std_msgs_msg_int32_get_type_name(), "",
                                           rclc_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
