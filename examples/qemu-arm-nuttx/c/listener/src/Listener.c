/// @file Listener.c
/// @brief NuttX C listener — typed component (RFC-0043, phase-240.4).
///
/// A stateful C component: `listener_configure` binds the `on_raw` callback (by
/// identity, fn-ptr + self ctx) as a raw zero-copy subscription on `/chatter`
/// and hand-decodes the `std_msgs/String` CDR (phase-277 W4 fallback: the
/// NuttX kernel link has no plumbing for the GENERATED typed C interface
/// sources yet, so this example keeps the platform's raw + hand-CDR shape).
/// Logs the official ROS 2 demo line (`I heard: [Hello World: N]`).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    int recv;
} listener_t;

static void on_raw(const uint8_t* data, size_t len, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    /* std_msgs/String CDR: 4-byte encapsulation header, u32 LE length
     * (payload incl. NUL), then the bytes. */
    if (len < 8) {
        return;
    }
    uint32_t n = (uint32_t)data[4] | ((uint32_t)data[5] << 8) | ((uint32_t)data[6] << 16) |
                 ((uint32_t)data[7] << 24);
    if (n == 0 || (size_t)n > len - 8) {
        return;
    }
    /* n includes the trailing NUL; print the text portion. */
    printf("I heard: [%.*s]\n", (int)(n - 1), (const char*)data + 8);
    self->recv++;
}

static nros_ret_t listener_configure(const nros_cpp_node_t* node, void* executor,
                                     listener_t* self) {
    (void)executor; /* node-scoped sub; executor unused */
    size_t handle;
    int32_t rc = nros_cpp_subscription_register(node, "/chatter", "std_msgs::msg::dds_::String_",
                                                "", nros_c_qos_default(), on_raw, self,
                                                /*sched_context=*/0, &handle,
                                                /*callback_group=*/NULL);
    if (rc == 0) {
        /* Readiness marker the rtos_e2e harness greps before driving the talker. */
        printf("Waiting for messages\n");
    }
    return rc;
}

NROS_C_COMPONENT(listener_t, listener_configure)
