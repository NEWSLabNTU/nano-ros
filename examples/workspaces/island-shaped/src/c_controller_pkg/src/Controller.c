/// @file Controller.c
/// @brief island-shaped workspace -- the C controller, a typed component (RFC-0043).
///
/// Shaped like the safety island's controller, at toy size: it subscribes to two
/// message types (`std_msgs/Int32` from the Rust monitor, `std_msgs/String`
/// status), publishes a command and a diagnostic, and runs a control timer.
/// Its parameters come from param files in THIS package's `config/`, which the
/// bringup's launch file loads with `<param from>` (issue 2615).

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <std_msgs/std_msgs.h>
#include <rclc/rclc.h>

typedef struct {
    rcl_subscription_t alive_sub;
    rcl_subscription_t status_sub;
    rcl_publisher_t cmd_pub;
    rcl_publisher_t diag_pub;
    rcl_timer_t timer;
    std_msgs__msg__Int32 alive;
    std_msgs__msg__String status;
    std_msgs__msg__String__Storage status_storage;
    int32_t ticks;
    int32_t last_alive;
} controller_t;

static void on_alive(const void* msgin, void* ctx) {
    controller_t* self = (controller_t*)ctx;
    self->last_alive = ((const std_msgs__msg__Int32*)msgin)->data;
}

static void on_status(const void* msgin, void* ctx) {
    (void)msgin;
    (void)ctx;
}

static void on_control(rcl_timer_t* timer, int64_t last_call_time) {
    controller_t* self = RCLC_CONTAINER_OF(timer, controller_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 cmd = {.data = self->ticks};
    std_msgs__msg__Int32 diag = {.data = self->last_alive};
    (void)rcl_publish(&self->cmd_pub, &cmd, NULL);
    (void)rcl_publish(&self->diag_pub, &diag, NULL);
    self->ticks++;
}

static rcl_ret_t controller_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      controller_t* self) {
    self->ticks = 0;
    self->last_alive = 0;
    std_msgs__msg__String__init_in(&self->status, &self->status_storage);

    self->alive_sub = rcl_get_zero_initialized_subscription();
    rcl_ret_t rc = rclc_subscription_init_default(
        &self->alive_sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/monitor/alive");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->status_sub = rcl_get_zero_initialized_subscription();
    rc = rclc_subscription_init_default(
        &self->status_sub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/status");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->cmd_pub = rcl_get_zero_initialized_publisher();
    rc = rclc_publisher_init_default(
        &self->cmd_pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/control/cmd");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->diag_pub = rcl_get_zero_initialized_publisher();
    rc = rclc_publisher_init_default(
        &self->diag_pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/control/diag");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 30ULL * 1000000ULL, on_control);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->alive_sub, &self->alive,
                                                     on_alive, self, ON_NEW_DATA);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_subscription_with_context(executor, &self->status_sub, &self->status,
                                                     on_status, self, ON_NEW_DATA);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(controller_t, controller_configure)
