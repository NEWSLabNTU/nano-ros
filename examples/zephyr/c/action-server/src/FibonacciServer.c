/// @file FibonacciServer.c
/// @brief Zephyr C Fibonacci action server — typed component.
///
/// `server_configure` creates an action server on `/fibonacci` with the C API
/// (`rclc_action_server_init_with_callbacks` + `rclc_executor_attach_action_server`).
/// The goal callback decodes the goal (int32 order); the accepted callback
/// computes the sequence and completes the goal with the result (int32[]
/// sequence) — `rclc_action_execute` + `rclc_action_succeed`. No interpreter.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

#include <example_interfaces/action/fibonacci.h>

typedef struct {
    rcl_action_server_t server;
    int32_t order;
} fib_server_t;

static rclc_goal_response_t on_goal(rcl_action_server_t* server, const rclc_goal_handle_t* goal,
                                    const uint8_t* data, size_t len, void* ctx) {
    (void)server;
    (void)goal;
    fib_server_t* self = (fib_server_t*)ctx;
    example_interfaces__action__Fibonacci_Goal req;
    if (example_interfaces__action__Fibonacci_Goal__deserialize(&req, data, len) != 0) {
        return RCLC_GOAL_REJECT;
    }
    self->order = req.order;
    printf("Received goal request with order %d\n", (int)self->order);
    return RCLC_GOAL_ACCEPT_AND_EXECUTE;
}

static rclc_cancel_response_t on_cancel(rcl_action_server_t* server, const rclc_goal_handle_t* goal,
                                        void* ctx) {
    (void)server;
    (void)goal;
    (void)ctx;
    return RCLC_CANCEL_REJECT;
}

static void on_accepted(rcl_action_server_t* server, const rclc_goal_handle_t* goal, void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    if (rclc_action_execute(server, goal) != RCL_RET_OK) {
        return;
    }
    printf("Executing goal\n");

    int32_t n = self->order;
    if (n < 0) {
        n = 0;
    }
    if (n > 16) {
        n = 16;
    }
    example_interfaces__action__Fibonacci_Result result;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    example_interfaces__action__Fibonacci_Result__init_in(&result, &result_storage);
    int32_t i;
    for (i = 0; i < n; ++i) {
        result.sequence.data[i] = (i == 0) ? 0
                                  : (i == 1)
                                      ? 1
                                      : result.sequence.data[i - 1] + result.sequence.data[i - 2];
    }
    result.sequence.size = (size_t)n;

    uint8_t buf[8 + 4 * 16];
    size_t result_len = 0;
    if (example_interfaces__action__Fibonacci_Result__serialize(&result, buf, sizeof(buf),
                                                                &result_len) != 0) {
        printf("Failed to complete goal (rc=%d)\n", (int)RCL_RET_ERROR);
        return;
    }
    rcl_ret_t rc = rclc_action_succeed(server, goal, buf, result_len);
    if (rc == RCL_RET_OK) {
        printf("Goal succeeded\n");
    } else {
        printf("Failed to complete goal (rc=%d)\n", (int)rc);
    }
}

static rcl_ret_t server_configure(rcl_node_t* node, rclc_executor_t* executor, fib_server_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->server = rcl_action_get_zero_initialized_server();
    rcl_ret_t rc = rclc_action_server_init_with_callbacks(
        &self->server, node, "/fibonacci",
        ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci), on_goal, on_cancel,
        on_accepted, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_attach_action_server(executor, &self->server);
    if (rc == RCL_RET_OK) {
        /* Readiness marker the e2e harness greps before sending a goal. */
        printf("Waiting for action goals\n");
    }
    return rc;
}

RCLC_C_COMPONENT(fib_server_t, server_configure)
