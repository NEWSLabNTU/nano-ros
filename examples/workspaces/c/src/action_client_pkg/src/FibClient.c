/// @file FibClient.c
/// @brief pure-C workspace — A4 actions: the Fibonacci action CLIENT, typed component
/// (RFC-0043 / phase-263). The C projection of the Rust `action_client_pkg`.
///
/// Async, with the C API (`rclc_action_send_goal_async` / `rclc_action_get_result_async` +
/// the goal-response and result callbacks), not a blocking call — a component callback must
/// never block the executor. A 500 ms timer tick pumps `rclc_action_client_poll` (which drains
/// the replies and dispatches them into the callbacks) and drives a small state machine:
///   idle → send_goal_async(order=10) → (goal-response cb: accepted) → get_result_async →
///   (result cb: deserialize the sequence + PRINT last element) → done.
/// A wait counter re-sends if a goal/result reply never arrives (the first request(s) can be
/// dropped before the server is discovered, like the A1 client's resend guard). The server
/// computes the order-10 Fibonacci sequence 0,1,1,2,3,5,8,13,21,34,55 → last = 55, so the
/// client printing `result last=55` proves the cross-process action round-trip.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <rclc/rclc.h>

#include "example_interfaces.h"

enum fib_phase {
    FIB_IDLE = 0,         /* no goal in flight — send one */
    FIB_GOAL_SENT = 1,    /* goal sent — await goal-response callback */
    FIB_NEED_RESULT = 2,  /* accepted — request the result from the tick */
    FIB_AWAIT_RESULT = 3, /* result requested — await result callback */
    FIB_DONE = 4          /* result received + printed */
};

typedef struct {
    rcl_action_client_t client;
    rcl_timer_t timer;
    rclc_goal_uuid_t goal_id;
    int phase;
    int waits; /* ticks waited in the current phase (resend guard) */
} fib_client_t;

/// Goal-response callback: on accept, advance to request the result; on
/// reject, fall back to idle to resend.
static void on_goal_response(const rclc_goal_uuid_t* goal_id, bool accepted, void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;
    if (accepted) {
        self->goal_id = *goal_id;
        self->phase = FIB_NEED_RESULT;
        self->waits = 0;
        printf("[action_client_pkg] goal accepted\n");
    } else {
        self->phase = FIB_IDLE;
    }
}

/// Result callback: deserialize the Fibonacci sequence and print its last
/// element — the cross-process round-trip proof.
static void on_result(const rclc_goal_uuid_t* goal_id, rclc_goal_status_t status,
                      const uint8_t* data, size_t len, void* ctx) {
    (void)goal_id;
    (void)status;
    fib_client_t* self = (fib_client_t*)ctx;

    example_interfaces__action__Fibonacci_Result result;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    example_interfaces__action__Fibonacci_Result__init_in(&result, &result_storage);
    if (data && len > 0 &&
        example_interfaces__action__Fibonacci_Result__deserialize(&result, data, len) == 0 &&
        result.sequence.size > 0) {
        printf("[action_client_pkg] result seq=[");
        for (uint32_t i = 0; i < result.sequence.size; i++) {
            printf(i > 0 ? ", %d" : "%d", result.sequence.data[i]);
        }
        printf("]\n");
        printf("[action_client_pkg] result last=%d\n",
               result.sequence.data[result.sequence.size - 1]);
        self->phase = FIB_DONE;
    }
}

static void send_goal(fib_client_t* self) {
    example_interfaces__action__Fibonacci_Goal goal = {.order = 10};
    uint8_t buf[64];
    size_t n = 0;
    if (example_interfaces__action__Fibonacci_Goal__serialize(&goal, buf, sizeof(buf), &n) == 0 &&
        rclc_action_send_goal_async(&self->client, buf, n, &self->goal_id) == RCL_RET_OK) {
        self->phase = FIB_GOAL_SENT;
        self->waits = 0;
    }
}

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    fib_client_t* self = RCLC_CONTAINER_OF(timer, fib_client_t, timer);
    (void)last_call_time;

    /* Pump pending replies -> fires the goal-response / result callbacks. */
    (void)rclc_action_client_poll(&self->client);

    switch (self->phase) {
    case FIB_IDLE:
        send_goal(self);
        break;
    case FIB_GOAL_SENT:
        if (++self->waits > 10) {
            self->phase = FIB_IDLE; /* no goal response — resend */
        }
        break;
    case FIB_NEED_RESULT:
        if (rclc_action_get_result_async(&self->client, &self->goal_id) == RCL_RET_OK) {
            self->phase = FIB_AWAIT_RESULT;
            self->waits = 0;
        }
        break;
    case FIB_AWAIT_RESULT:
        if (++self->waits > 40) {
            self->phase = FIB_NEED_RESULT; /* no result — re-request */
        }
        break;
    case FIB_DONE:
    default:
        break;
    }
}

static rcl_ret_t fib_client_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      fib_client_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->phase = FIB_IDLE;
    self->waits = 0;

    self->client = rcl_action_get_zero_initialized_client();
    rcl_ret_t rc = rcl_action_client_init(
        &self->client, node, ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci),
        "/fibonacci", NULL);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_action_client_set_goal_response_callback(&self->client, on_goal_response, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_action_client_set_result_callback(&self->client, on_result, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_attach_action_client(executor, &self->client);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 500ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(fib_client_t, fib_client_configure)
