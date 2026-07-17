/// @file FibClient.c
/// @brief pure-C workspace — A4 actions: the Fibonacci action CLIENT, typed component
/// (RFC-0043 / phase-263). The C projection of the Rust `action_client_pkg`.
///
/// POLL-model (`nros_cpp_action_client_{send_goal_async,get_result_async,poll}` +
/// `set_callbacks`), not a blocking call — a component callback must never block the executor.
/// A 500 ms timer tick pumps `nros_cpp_action_client_poll` (which drains the GET-query replies
/// and dispatches them into the registered callbacks) and drives a small state machine:
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

#include <nros/component.h>

#include "example_interfaces.h"

enum fib_phase {
    FIB_IDLE = 0,         /* no goal in flight — send one */
    FIB_GOAL_SENT = 1,    /* goal sent — await goal-response callback */
    FIB_NEED_RESULT = 2,  /* accepted — request the result from the tick */
    FIB_AWAIT_RESULT = 3, /* result requested — await result callback */
    FIB_DONE = 4          /* result received + printed */
};

typedef struct {
    _Alignas(8) uint8_t client[NROS_C_ACTION_CLIENT_STORAGE_SIZE];
    uint8_t goal_id[16];
    int phase;
    int waits; /* ticks waited in the current phase (resend guard) */
} fib_client_t;

/// Goal-response callback (fired from poll()): on accept, advance to request the result; on
/// reject, fall back to idle to resend.
static void on_goal_response(bool accepted, const uint8_t goal_id[16], void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;
    if (accepted) {
        memcpy(self->goal_id, goal_id, 16);
        self->phase = FIB_NEED_RESULT;
        self->waits = 0;
        printf("[c_fib_client_pkg] goal accepted\n");
    } else {
        self->phase = FIB_IDLE;
    }
}

/// Result callback (fired from poll()): deserialize the Fibonacci sequence and print its last
/// element — the cross-process round-trip proof.
static void on_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len,
                      void* ctx) {
    (void)goal_id;
    (void)status;
    fib_client_t* self = (fib_client_t*)ctx;

    example_interfaces_action_fibonacci_result result;
    if (data && len > 0 &&
        example_interfaces_action_fibonacci_result_deserialize(&result, data, len) == 0 &&
        result.sequence.size > 0) {
        printf("[c_fib_client_pkg] result seq=[");
        for (uint32_t i = 0; i < result.sequence.size; i++) {
            printf(i > 0 ? ", %d" : "%d", result.sequence.data[i]);
        }
        printf("]\n");
        printf("[c_fib_client_pkg] result last=%d\n",
               result.sequence.data[result.sequence.size - 1]);
        self->phase = FIB_DONE;
    }
}

static void send_goal(fib_client_t* self) {
    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;
    uint8_t buf[64];
    size_t n = 0;
    int32_t n_rc = example_interfaces_action_fibonacci_goal_serialize(&goal, buf, sizeof(buf), &n);
    if (n_rc == 0 &&
        nros_cpp_action_client_send_goal_async(self->client, buf, n, &self->goal_id) == 0) {
        self->phase = FIB_GOAL_SENT;
        self->waits = 0;
    }
}

static void on_tick(void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;

    /* Pump pending replies → fires the goal-response / result callbacks. */
    nros_cpp_action_client_poll(self->client);

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
        if (nros_cpp_action_client_get_result_async(self->client, &self->goal_id) == 0) {
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

static nros_ret_t fib_client_configure(const nros_cpp_node_t* node, void* executor,
                                       fib_client_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->phase = FIB_IDLE;
    self->waits = 0;

    int32_t rc = nros_cpp_action_client_create(
        node, "/fibonacci", example_interfaces_action_fibonacci_get_type_name(),
        example_interfaces_action_fibonacci_get_type_hash(), nros_c_qos_default(), self->client);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_action_client_set_callbacks(self->client, on_goal_response, /*feedback=*/NULL,
                                              on_result, self);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(fib_client_t, fib_client_configure)
