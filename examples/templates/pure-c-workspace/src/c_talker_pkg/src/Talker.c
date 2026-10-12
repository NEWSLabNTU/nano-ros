/// @file Talker.c
/// @brief pure-C workspace — C talker, typed component (RFC-0043 / phase-257).
///
/// `talker_configure` creates a publisher on `/chatter` + a 1 Hz timer that
/// publishes an Int32 counter, with the C API. `RCLC_C_COMPONENT` emits the C-ABI
/// factory/configure the typed C Entry calls; it runs on the real executor
/// (nros_board_native_run_components) — no declarative descriptor, no interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <nros/component.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, void* ctx) {
    (void)timer;
    talker_t* self = (talker_t*)ctx;
    std_msgs__msg__Int32 msg = {.data = self->count};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("[c_talker_pkg] sent: %d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_with_context(&self->timer, 1000ULL * 1000000ULL, on_tick, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
