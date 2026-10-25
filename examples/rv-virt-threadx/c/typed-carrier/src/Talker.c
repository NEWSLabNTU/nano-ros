/// @file Talker.c
/// @brief rv-virt-threadx — a C talker deployed through the ThreadX TYPED
/// CARRIER (`nros_components_register_node(... TYPED DEPLOY threadx)`), the
/// road `nano_ros_entry()` does not take (issue 1795). Same component as
/// `examples/templates/multi-package-workspace/src/pkg_c_talker`.
///
/// `talker_configure` creates a publisher on `/chatter` + a 1 Hz timer that
/// publishes an Int32 counter, with the C API. `RCLC_C_COMPONENT` emits the C-ABI
/// factory/configure the native typed Entry carrier (the shared entry pack)
/// calls; the carrier runs it on the real executor (NativeBoard::run_components) —
/// no hand-written `main`, no `rclc_support_init`/`node_init`/
/// `executor_init`/spin-loop boilerplate (issue-0049 P1/P7).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg = {.data = self->count};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("[typed_carrier] sent: %d\n", (int)self->count);
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
    rc = rclc_timer_init_default(&self->timer, NULL, 1000ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
