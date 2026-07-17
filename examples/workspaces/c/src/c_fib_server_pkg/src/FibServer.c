/// @file FibServer.c
/// @brief pure-C workspace — A4 actions: the Fibonacci action SERVER, typed component
/// (RFC-0043 / phase-263). The C projection of the Rust `action_server_pkg`.
///
/// `fib_server_configure` creates + registers a raw callback-style action server on
/// `/fibonacci` via the executor-scoped component seams (`nros_cpp_action_server_create` /
/// `_register` / `_set_callbacks`). The goal callback parses the Fibonacci goal `{int32 order}`
/// using the generated `example_interfaces/action/Fibonacci` C bindings, ACCEPTS it, and
/// stashes the goal_id + order in the component state. A 500 ms timer tick computes the
/// sequence and completes the goal via `nros_cpp_action_server_complete_goal`. A component
/// callback must never block the executor, so the compute/complete runs from the timer — the
/// only place the executor is free for action ops (mirrors the Rust `tick()` shape).
///
/// Cross-process only (issue 0096): an in-process (same-executor) server+client can't talk, so
/// the server runs as its OWN entry (`native_action_server_entry`) and the client as another.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

// Generated C bindings for example_interfaces/action/Fibonacci
// (nros_find_interfaces(LANGUAGE C) → example_interfaces__nano_ros_c).
#include "example_interfaces.h"

typedef struct {
    _Alignas(8) uint8_t server[NROS_C_ACTION_SERVER_STORAGE_SIZE];
    void* executor;      /* opaque executor handle (needed for complete_goal) */
    uint8_t goal_id[16]; /* the accepted goal's UUID */
    int32_t order;       /* the requested Fibonacci order */
    bool has_pending;    /* a goal is accepted + awaiting its tick-driven result */
    int goal_count;
} fib_server_t;

/// Goal callback — receives the goal UUID + the goal's CDR bytes. Parses the order, ACCEPTS,
/// and stashes the goal for the timer to complete. ABI-identical to the C++ goal trampoline.
static int32_t fib_server_on_goal(const uint8_t goal_id[16], const uint8_t* data, size_t len,
                                  void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;

    example_interfaces_action_fibonacci_goal goal;
    if (example_interfaces_action_fibonacci_goal_deserialize(&goal, data, len) != 0) {
        return NROS_C_GOAL_REJECT;
    }
    if (goal.order < 0 || goal.order >= 64) {
        printf("[c_fib_server_pkg] goal order=%d REJECTED (out of range)\n", goal.order);
        return NROS_C_GOAL_REJECT;
    }

    memcpy(self->goal_id, goal_id, 16);
    self->order = goal.order;
    self->has_pending = true;
    printf("[c_fib_server_pkg] goal order=%d\n", goal.order);
    return NROS_C_GOAL_ACCEPT_AND_EXECUTE;
}

static int32_t fib_server_on_cancel(const uint8_t goal_id[16], void* ctx) {
    (void)goal_id;
    (void)ctx;
    return NROS_C_CANCEL_ACCEPT;
}

/// Timer tick — the only place the executor is free for action ops. Computes the Fibonacci
/// sequence for the accepted goal and completes it with the result CDR.
static void on_tick(void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    if (!self->has_pending) {
        return;
    }

    example_interfaces_action_fibonacci_result result;
    example_interfaces_action_fibonacci_result_init(&result);
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
    int32_t n_rc =
        example_interfaces_action_fibonacci_result_serialize(&result, buf, sizeof(buf), &n);
    if (n_rc != 0) {
        return;
    }
    if (nros_cpp_action_server_complete_goal(self->server, self->executor, &self->goal_id, buf,
                                             n) == 0) {
        self->has_pending = false;
        self->goal_count++;
        printf("[c_fib_server_pkg] completed last=%d\n",
               result.sequence.data[result.sequence.size - 1]);
    }
}

static nros_ret_t fib_server_configure(const nros_cpp_node_t* node, void* executor,
                                       fib_server_t* self) {
    /* Line-buffer stdout so each line flushes when piped to the test harness. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->executor = executor;
    self->has_pending = false;
    self->goal_count = 0;

    int32_t rc = nros_cpp_action_server_create(
        node, "/fibonacci", example_interfaces_action_fibonacci_get_type_name(),
        example_interfaces_action_fibonacci_get_type_hash(), nros_c_qos_default(), self->server);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_action_server_register(self->server, executor, "/fibonacci",
                                         example_interfaces_action_fibonacci_get_type_name(),
                                         example_interfaces_action_fibonacci_get_type_hash(),
                                         /*sched_context=*/0);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_action_server_set_callbacks(self->server, fib_server_on_goal,
                                              fib_server_on_cancel, self);
    if (rc != 0) {
        return rc;
    }

    size_t timer_handle;
    rc = nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
    if (rc == 0) {
        printf("[c_fib_server_pkg] fibonacci action server ready\n");
    }
    return rc;
}

NROS_C_COMPONENT(fib_server_t, fib_server_configure)
