/// @file FibServer.c
/// @brief pure-C workspace — A4 actions: the Fibonacci action SERVER, typed component
/// (RFC-0043 / phase-263). The C projection of the Rust `action_server_pkg`.
///
/// `fib_server_configure` creates an action server on `/fibonacci` in rclc's shape
/// (`rclc_action_server_init_default` + `rclc_executor_add_action_server`). The goal callback
/// reads the order out of the deserialised `Fibonacci_SendGoal_Request` and ACCEPTS it; a
/// component has no main loop, so a timer computes the sequence and completes the goal with
/// `rclc_action_send_result`, publishing each element as feedback on the way.
///
/// phase-535 (RFC-0122 option B): this is the ONE C Fibonacci server — the standalone example
/// pairs and the workspace images run the same program, printing the tutorial server's
/// `Received goal request`, `Executing goal`, `Publish feedback` and `Goal succeeded` lines.
///
/// Cross-process only (issue 0096): an in-process (same-executor) server+client can't talk, so
/// the server runs as its OWN entry (`linux_action_server_entry`) and the client as another.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

// Generated C bindings for example_interfaces/action/Fibonacci
// (nros_find_interfaces(LANGUAGE C) → example_interfaces__nano_ros_c).
#include "example_interfaces.h"

typedef struct {
    rclc_action_server_t server;
    rcl_timer_t timer;
    /* One goal handle: its request lands here. */
    example_interfaces__action__Fibonacci_SendGoal_Request goal_request;
    /* The accepted goal the timer has not run yet. */
    rclc_action_goal_handle_t* pending;
    int goal_count;
} fib_server_t;

/// Goal callback — the request is already deserialised into `goal_request`.
static rcl_ret_t fib_server_on_goal(rclc_action_goal_handle_t* goal_handle, void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    const example_interfaces__action__Fibonacci_SendGoal_Request* req =
        (const example_interfaces__action__Fibonacci_SendGoal_Request*)
            goal_handle->ros_goal_request;
    if (req->goal.order < 0 || req->goal.order >= 64) {
        printf("Goal rejected: order out of range\n");
        return RCL_RET_ACTION_GOAL_REJECTED;
    }
    printf("Received goal request with order %d\n", req->goal.order);
    self->pending = goal_handle;
    return RCL_RET_ACTION_GOAL_ACCEPTED;
}

static bool fib_server_on_cancel(rclc_action_goal_handle_t* goal_handle, void* ctx) {
    (void)goal_handle;
    (void)ctx;
    return true;
}

/// Timer — compute the sequence the client asked for and complete the goal.
static void fib_server_on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    (void)last_call_time;
    fib_server_t* self = RCLC_CONTAINER_OF(timer, fib_server_t, timer);
    rclc_action_goal_handle_t* goal_handle = self->pending;
    if (goal_handle == NULL) {
        return;
    }
    self->pending = NULL;

    printf("Executing goal\n");

    example_interfaces__action__Fibonacci_FeedbackMessage feedback;
    example_interfaces__action__Fibonacci_Feedback__Storage fb_storage;
    example_interfaces__action__Fibonacci_Feedback__init_in(&feedback.feedback, &fb_storage);
    example_interfaces__action__Fibonacci_Feedback* fb = &feedback.feedback;
    for (int32_t i = 0; i <= self->goal_request.goal.order; i++) {
        if (goal_handle->goal_cancelled) {
            break;
        }
        int32_t val;
        if (i == 0) {
            val = 0;
        } else if (i == 1) {
            val = 1;
        } else {
            val = fb->sequence.data[i - 1] + fb->sequence.data[i - 2];
        }
        fb->sequence.data[i] = val;
        fb->sequence.size = (uint32_t)(i + 1);
        /* rclc_action_publish_feedback writes the goal id into the message. */
        rcl_ret_t frc = rclc_action_publish_feedback(goal_handle, &feedback);
        if (frc != RCL_RET_OK) {
            fprintf(stderr, "Failed to publish feedback: %d\n", (int)frc);
        } else {
            printf("Publish feedback\n");
        }
    }

    example_interfaces__action__Fibonacci_GetResult_Response response;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    example_interfaces__action__Fibonacci_Result__init_in(&response.result, &result_storage);
    response.result.sequence.size = fb->sequence.size;
    memcpy(response.result.sequence.data, fb->sequence.data, fb->sequence.size * sizeof(int32_t));

    rcl_action_goal_state_t status =
        goal_handle->goal_cancelled ? GOAL_STATE_CANCELED : GOAL_STATE_SUCCEEDED;
    rcl_ret_t rc = rclc_action_send_result(goal_handle, status, &response);
    if (rc != RCL_RET_OK) {
        /* issue 0737 — say why. A silent failure is indistinguishable from
         * a goal that never arrived. */
        fprintf(stderr, "Failed to send result: %d\n", (int)rc);
    } else if (status == GOAL_STATE_SUCCEEDED) {
        self->goal_count++;
        printf("Goal succeeded\n");
    } else {
        printf("Goal canceled\n");
    }
}

static rcl_ret_t fib_server_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      fib_server_t* self) {
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    /* Line-buffer stdout so each line flushes when piped to the test harness. */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    self->goal_count = 0;
    self->pending = NULL;
    self->server = rcl_action_get_zero_initialized_server();
    /* A component has no support to pass; NULL is accepted, as for timers. */
    rcl_ret_t rc = rclc_action_server_init_default(
        &self->server, node, NULL,
        ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci), "/fibonacci");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_action_server(executor, &self->server, 1, &self->goal_request,
                                         sizeof(self->goal_request), fib_server_on_goal,
                                         fib_server_on_cancel, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 20ULL * 1000000ULL, fib_server_on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_add_timer(executor, &self->timer);
    if (rc == RCL_RET_OK) {
        printf("Waiting for action goals\n");
    }
    return rc;
}

RCLC_C_COMPONENT(fib_server_t, fib_server_configure)
