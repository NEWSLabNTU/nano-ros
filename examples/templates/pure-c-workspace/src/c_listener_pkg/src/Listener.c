/// Listener — typed C component (RFC-0043, phase-247).
///
/// `listener_configure` registers a raw (zero-copy) subscription on `/chatter`
/// matching the DDS-mangled `std_msgs::msg::dds_::Int32_` keyexpr the talker
/// publishes. The callback borrows the wire bytes (no copy/deserialize), decodes
/// the CDR-encoded Int32, and counts receipts. Replaces the legacy
/// `register_listener` + record_callback_effect declarative seam. The Entry pkg's
/// typed carrier constructs this component via the
/// __nros_c_component_c_listener_pkg_{create,configure} seam.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    int32_t recv;
} listener_t;

static void on_message(const uint8_t* data, size_t len, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    /* CDR-encoded std_msgs/Int32: 4-byte encapsulation header, then the LE i32. */
    int32_t v = 0;
    if (len >= 8) {
        v = (int32_t)((uint32_t)data[4] | ((uint32_t)data[5] << 8) | ((uint32_t)data[6] << 16) |
                      ((uint32_t)data[7] << 24));
    }
    printf("Received: %d\n", (int)v);
    self->recv++;
}

static nros_ret_t listener_configure(const nros_cpp_node_t* node, void* executor, listener_t* self) {
    (void)executor;
    setvbuf(stdout, NULL, _IONBF, 0);
    self->recv = 0;
    size_t handle;
    return nros_cpp_subscription_register(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                          nros_c_qos_default(), on_message, self,
                                          /*sched_context=*/0, &handle);
}

NROS_C_COMPONENT(listener_t, listener_configure)
