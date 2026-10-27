/// @file FibonacciServer.c
/// @brief Zephyr C Fibonacci action server — typed component.
///
/// `server_configure` creates an action server on `/fibonacci` in rclc's shape
/// (`rclc_action_server_init_default` + `rclc_executor_add_action_server`).
/// The goal callback reads the order out of the deserialised
/// `Fibonacci_SendGoal_Request` and accepts; a component has no main loop, so
/// a 20 ms timer runs the accepted goal and answers it with
/// `rclc_action_send_result`. No interpreter.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

#include <example_interfaces/action/fibonacci.h>

typedef struct {
    rclc_action_server_t server;
    rcl_timer_t timer;
    // One goal handle: its request lands here.
    example_interfaces__action__Fibonacci_SendGoal_Request goal_request;
    // The accepted goal the timer has not run yet.
    rclc_action_goal_handle_t* pending;
} fib_server_t;

static rcl_ret_t on_goal(rclc_action_goal_handle_t* goal_handle, void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    const example_interfaces__action__Fibonacci_SendGoal_Request* req =
        (const example_interfaces__action__Fibonacci_SendGoal_Request*)
            goal_handle->ros_goal_request;
    printf("Received goal request with order %d\n", (int)req->goal.order);
    self->pending = goal_handle;
    return RCL_RET_ACTION_GOAL_ACCEPTED;
}

static bool on_cancel(rclc_action_goal_handle_t* goal_handle, void* ctx) {
    (void)goal_handle;
    (void)ctx;
    return false;
}

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    (void)last_call_time;
    fib_server_t* self = RCLC_CONTAINER_OF(timer, fib_server_t, timer);
    rclc_action_goal_handle_t* goal_handle = self->pending;
    if (goal_handle == NULL) {
        return;
    }
    self->pending = NULL;
    printf("Executing goal\n");

    int32_t n = self->goal_request.goal.order;
    if (n < 0) {
        n = 0;
    }
    if (n > 16) {
        n = 16;
    }
    example_interfaces__action__Fibonacci_GetResult_Response response;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    example_interfaces__action__Fibonacci_Result__init_in(&response.result, &result_storage);
    example_interfaces__action__Fibonacci_Result* result = &response.result;
    int32_t i;
    for (i = 0; i < n; ++i) {
        result->sequence.data[i] =
            (i == 0)   ? 0
            : (i == 1) ? 1
                       : result->sequence.data[i - 1] + result->sequence.data[i - 2];
    }
    result->sequence.size = (size_t)n;

    rcl_ret_t rc = rclc_action_send_result(goal_handle, GOAL_STATE_SUCCEEDED, &response);
    if (rc == RCL_RET_OK) {
        printf("Goal succeeded\n");
    } else {
        printf("Failed to complete goal (rc=%d)\n", (int)rc);
    }
}

static rcl_ret_t server_configure(rcl_node_t* node, rclc_executor_t* executor, fib_server_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->pending = NULL;
    self->server = rcl_action_get_zero_initialized_server();
    // A component has no support to pass; NULL is accepted, as for timers.
    rcl_ret_t rc = rclc_action_server_init_default(
        &self->server, node, NULL,
        ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci), "/fibonacci");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_action_server(executor, &self->server, 1, &self->goal_request,
                                         sizeof(self->goal_request), on_goal, on_cancel, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 20ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_timer(executor, &self->timer);
    if (rc == RCL_RET_OK) {
        /* Readiness marker the e2e harness greps before sending a goal. */
        printf("Waiting for action goals\n");
    }
    return rc;
}

RCLC_C_COMPONENT(fib_server_t, server_configure)
