/// @file Listener.c
/// @brief pure-C workspace — the C listener, a typed component (RFC-0043 / phase-257).
///
/// The ROS 2 tutorial listener: subscribes to `std_msgs/String` on `/chatter` and prints
/// `I heard: [<payload>]` for each sample. phase-535 made this the ONE C listener: the
/// standalone example pairs and the workspace images run the same program (RFC-0122 D1,
/// option B). `RCLC_C_COMPONENT` emits the C-ABI factory/configure the generated entry calls.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_subscription_t sub;
    std_msgs__msg__String msg;
    // The bytes `msg` decodes into: the type's bound comes from the workspace's
    // `nros-codegen.toml` (RFC-0033), and a longer sample is refused, never truncated.
    std_msgs__msg__String__Storage msg_storage;
    int recv;
} listener_t;

/* issue 0737 — a sample that fails to decode never reaches this callback; the
 * executor drops it, counts it in the spin result and LOGS it (rate-limited),
 * so a rejected sample is never indistinguishable from one that never arrived. */
static void on_msg(const void* msgin, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    const std_msgs__msg__String* msg = (const std_msgs__msg__String*)msgin;
    self->recv++;
    printf("I heard: [%s]\n", msg->data.data);
}

static rcl_ret_t listener_configure(rcl_node_t* node, rclc_executor_t* executor, listener_t* self) {
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    /* Line-buffer stdout so each line flushes immediately when piped (an
     * external test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    self->recv = 0;
    std_msgs__msg__String__init_in(&self->msg, &self->msg_storage);
    self->sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    printf("Subscriber created for topic: %s\n", rcl_subscription_get_topic_name(&self->sub));
    rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg, on_msg, self,
                                                     ON_NEW_DATA);
    if (rc == RCL_RET_OK) {
        printf("Waiting for messages\n");
    }
    return rc;
}

RCLC_C_COMPONENT(listener_t, listener_configure)
