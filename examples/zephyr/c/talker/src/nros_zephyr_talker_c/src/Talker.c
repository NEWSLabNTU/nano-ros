/// @file Talker.c
/// @brief Zephyr C talker — typed component (RFC-0043 / phase-244.C2).
///
/// `talker_configure` creates a publisher on `/chatter` + a timer that
/// publishes the official ROS 2 demo payload (`std_msgs/String`,
/// `Hello World: N`) each tick, via the GENERATED typed C bindings
/// (`nros_find_interfaces(LANGUAGE C)` in CMakeLists — phase-277 W4; was
/// hand-rolled CDR). `RCLC_C_COMPONENT` emits the C-ABI factory/configure the
/// Zephyr typed Entry carrier (the shared entry pack) calls.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

#include <std_msgs/msg/string.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
    /* The message lives in the component (not the tick stack): String is
     * ~256 B and RTOS timer/app stacks are small. */
    std_msgs__msg__String msg;
    /* The bytes behind `msg.data` (phase-492 W7), wired once in configure. */
    std_msgs__msg__String__Storage msg_storage;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    /* Pre-increment so the first payload is "Hello World: 1", matching the
     * official ROS 2 demo talker. */
    self->count++;
    snprintf(self->msg.data.data, self->msg.data.capacity, "Hello World: %d", (int)self->count);
    self->msg.data.size = strlen(self->msg.data.data);
    /* `rcl_publish` serializes into a buffer bounded at build time from the
     * type's declared size (phase-492 W8). */
    if (rcl_publish(&self->pub, &self->msg, NULL) == RCL_RET_OK) {
        printf("Publishing: '%s'\n", self->msg.data.data);
    }
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->count = 0;
    std_msgs__msg__String__init_in(&self->msg, &self->msg_storage);
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rcl_ret_t trc = rclc_timer_init_default(&self->timer, NULL, 500ULL * 1000000ULL, on_tick);
    if (trc != RCL_RET_OK) {
        return trc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
