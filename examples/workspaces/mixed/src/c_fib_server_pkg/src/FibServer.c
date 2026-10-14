/// @file FibServer.c
/// @brief pure-C workspace — A4 actions: the Fibonacci action SERVER, typed component
/// (RFC-0043 / phase-263). The C projection of the Rust `action_server_pkg`.
///
/// `fib_server_configure` creates an action server on `/fibonacci` with the C API
/// (`rclc_action_server_init_with_callbacks` + `rclc_executor_attach_action_server`). The goal
/// callback parses the Fibonacci goal `{int32 order}` with the generated
/// `example_interfaces/action/Fibonacci` C bindings and ACCEPTS it; the accepted callback
/// computes the sequence and completes the goal (`rclc_action_execute` + `rclc_action_succeed`),
/// as `examples/linux/c/action-server` does.
///
/// Cross-process only (issue 0096): an in-process (same-executor) server+client can't talk, so
/// the server runs as its OWN entry (`linux_action_server_entry`) and the client as another.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

// Generated C bindings for example_interfaces/action/Fibonacci
// (nros_find_interfaces(LANGUAGE C) → example_interfaces__nano_ros_c).
#include "example_interfaces.h"

typedef struct {
    rcl_action_server_t server;
    int32_t order; /* the accepted goal's Fibonacci order */
    int goal_count;
} fib_server_t;

/// Goal callback — the goal's CDR bytes. Parses the order and decides.
static rclc_goal_response_t fib_server_on_goal(rcl_action_server_t* server,
                                               const rclc_goal_handle_t* goal, const uint8_t* data,
                                               size_t len, void* ctx) {
    (void)server;
    (void)goal;
    fib_server_t* self = (fib_server_t*)ctx;

    example_interfaces__action__Fibonacci_Goal req;
    if (example_interfaces__action__Fibonacci_Goal__deserialize(&req, data, len) != 0) {
        /* issue 0737 — say why. A silent reject is indistinguishable from
         * a request that never arrived. */
        fprintf(stderr, "[c_fib_server_pkg] goal REJECTED — deserialize failed, %zu byte(s)\n",
                len);
        return RCLC_GOAL_REJECT;
    }
    if (req.order < 0 || req.order >= 64) {
        printf("[c_fib_server_pkg] goal order=%d REJECTED (out of range)\n", req.order);
        return RCLC_GOAL_REJECT;
    }
    self->order = req.order;
    printf("[c_fib_server_pkg] goal order=%d\n", req.order);
    return RCLC_GOAL_ACCEPT_AND_EXECUTE;
}

static rclc_cancel_response_t fib_server_on_cancel(rcl_action_server_t* server,
                                                   const rclc_goal_handle_t* goal, void* ctx) {
    (void)server;
    (void)goal;
    (void)ctx;
    return RCLC_CANCEL_ACCEPT;
}

/// Accepted — compute the sequence the client asked for and complete the goal.
static void fib_server_on_accepted(rcl_action_server_t* server, const rclc_goal_handle_t* goal,
                                   void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    if (rclc_action_execute(server, goal) != RCL_RET_OK) {
        return;
    }

    example_interfaces__action__Fibonacci_Result result;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    example_interfaces__action__Fibonacci_Result__init_in(&result, &result_storage);
    for (int32_t i = 0; i <= self->order; i++) {
        int32_t val;
        if (i == 0) {
            val = 0;
        } else if (i == 1) {
            val = 1;
        } else {
            val = result.sequence.data[i - 1] + result.sequence.data[i - 2];
        }
        result.sequence.data[i] = val;
        result.sequence.size = (uint32_t)(i + 1);
    }

    uint8_t buf[512];
    size_t n = 0;
    if (example_interfaces__action__Fibonacci_Result__serialize(&result, buf, sizeof(buf), &n) !=
        0) {
        return;
    }
    if (rclc_action_succeed(server, goal, buf, n) == RCL_RET_OK) {
        self->goal_count++;
        printf("[c_fib_server_pkg] completed last=%d\n",
               result.sequence.data[result.sequence.size - 1]);
    }
}

static rcl_ret_t fib_server_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      fib_server_t* self) {
    /* Line-buffer stdout so each line flushes when piped to the test harness. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->goal_count = 0;
    self->server = rcl_action_get_zero_initialized_server();
    rcl_ret_t rc = rclc_action_server_init_with_callbacks(
        &self->server, node, "/fibonacci",
        ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci), fib_server_on_goal,
        fib_server_on_cancel, fib_server_on_accepted, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_attach_action_server(executor, &self->server);
    if (rc == RCL_RET_OK) {
        printf("[c_fib_server_pkg] fibonacci action server ready\n");
    }
    return rc;
}

RCLC_C_COMPONENT(fib_server_t, fib_server_configure)
