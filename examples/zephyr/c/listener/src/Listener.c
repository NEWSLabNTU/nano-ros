/// @file Listener.c
/// @brief Zephyr C listener — typed component (RFC-0043 / phase-244.C2).
///
/// A stateful C component: `listener_configure` binds the `on_msg` callback (by
/// identity, fn-ptr + self ctx) to a subscription on `/chatter` with the C API;
/// the executor decodes each sample with the GENERATED typed C deserializer
/// (`nros_find_interfaces(LANGUAGE C)` in CMakeLists — phase-277 W4; was
/// hand-rolled CDR) and logging the official ROS 2 demo line
/// (`I heard: [Hello World: N]`). `RCLC_C_COMPONENT` emits the C-ABI factory +
/// configure the Zephyr typed Entry carrier (the shared entry pack)
/// calls. No declarative descriptor, no synthesizing interpreter, no callback
/// name.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

#include <std_msgs/msg/string.h>

typedef struct {
    rcl_subscription_t sub;
    int recv;
    /* Decoded message lives in the component (not the callback stack):
     * String is ~256 B and RTOS dispatch stacks are small. */
    std_msgs__msg__String msg;
    /* The bytes behind `msg.data` (phase-492 W7), wired once in configure;
     * a longer sample is refused, never truncated. */
    std_msgs__msg__String__Storage msg_storage;
} listener_t;

/* issue 0737 — a sample that fails to decode never reaches this callback: the
 * executor drops it, counts it and LOGS it (rate-limited), so a rejected
 * sample is never indistinguishable from one that never arrived. */
static void on_msg(const void* msgin, void* ctx) {
    listener_t* self = (listener_t*)ctx;
    const std_msgs__msg__String* msg = (const std_msgs__msg__String*)msgin;
    printf("I heard: [%s]\n", msg->data.data);
    self->recv++;
}

static rcl_ret_t listener_configure(rcl_node_t* node, rclc_executor_t* executor, listener_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    std_msgs__msg__String__init_in(&self->msg, &self->msg_storage);
    self->sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_default(
        &self->sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->sub, &self->msg, on_msg, self,
                                                     ON_NEW_DATA);
    if (rc == RCL_RET_OK) {
        /* Readiness markers the harness greps before driving the talker.
         * The first is the SHARED one every nano-ros listener prints
         * (`nros_tests::output::LISTENER_READY_MARKER`, phase-342 W7); the
         * second is zephyr.rs's own `NODE_READY_MARKER` and stays until that
         * file moves onto the shared table. */
        printf("Subscriber created for topic: %s\n", "/chatter");
        printf("Waiting for messages\n");
    }
    return rc;
}

RCLC_C_COMPONENT(listener_t, listener_configure)
