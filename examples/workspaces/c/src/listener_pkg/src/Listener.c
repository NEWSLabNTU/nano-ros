/// @file Listener.c
/// @brief pure-C workspace — C listener, typed component (RFC-0043 / phase-257).
///
/// `listener_configure` subscribes to `/chatter` with the C API and binds
/// `on_msg` (by identity, fn-ptr + self ctx) with the executor (phase-492 W4). `RCLC_C_COMPONENT`
/// emits the C-ABI factory/configure the typed C Entry calls. No declarative descriptor, no
/// interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <nros/component.h>

typedef struct {
    rcl_subscription_t sub;
    std_msgs__msg__Int32 msg;
    int recv;
} listener_t;

/* issue 0737 — a sample that fails to decode never reaches this callback; the
 * executor drops it, counts it in the spin result and LOGS it (rate-limited),
 * so a rejected sample is never indistinguishable from one that never arrived. */
static void on_msg(const void* msgin, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    const std_msgs__msg__Int32* msg = (const std_msgs__msg__Int32*)msgin;
    printf("Received: %d\n", (int)msg->data);
    self->recv++;
}

static rcl_ret_t listener_configure(rcl_node_t* node, rclc_executor_t* executor, listener_t* self) {
    /* Line-buffer stdout so each `Received:` flushes immediately when piped (an
     * external test reads the output live; glibc block-buffers a pipe otherwise). */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->recv = 0;
    self->sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg, on_msg, self,
                                                     RCLC_EXECUTOR_ON_NEW_DATA);
    if (rc == RCL_RET_OK) {
        printf("Waiting for messages\n");
    }
    return rc;
}

RCLC_C_COMPONENT(listener_t, listener_configure)
