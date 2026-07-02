/// @file ActionServer.c
/// @brief ThreadX-Linux C Fibonacci action server — typed component (RFC-0043).
///
/// `server_configure` binds goal/cancel callbacks (by identity) as a raw action
/// server on `/fibonacci`, and a timer that drives goal execution: decode the CDR
/// goal (int32 order), compute the sequence, complete with a CDR result.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t storage[NROS_C_ACTION_SERVER_STORAGE_SIZE];
    void* executor;
    int pending;
    uint8_t goal_id[16];
    int32_t order;
} fib_server_t;

static int32_t read_i32_le(const uint8_t* p) {
    return (int32_t)((uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) |
                     ((uint32_t)p[3] << 24));
}

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static int32_t on_goal(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    if (len < 8 || self->pending) {
        return NROS_C_GOAL_REJECT;
    }
    memcpy(self->goal_id, goal_id, 16);
    self->order = read_i32_le(data + 4);
    self->pending = 1;
    printf("Received goal request with order %d\n", (int)self->order);
    return NROS_C_GOAL_ACCEPT_AND_EXECUTE;
}

static int32_t on_cancel(const uint8_t goal_id[16], void* ctx) {
    (void)goal_id;
    (void)ctx;
    return NROS_C_CANCEL_REJECT;
}

static void on_tick(void* ctx) {
    fib_server_t* self = (fib_server_t*)ctx;
    if (!self->pending) {
        return;
    }
    self->pending = 0;
    printf("Executing goal\n");

    int32_t n = self->order;
    if (n < 0) {
        n = 0;
    }
    if (n > 16) {
        n = 16;
    }
    int32_t seq[16];
    int32_t i;
    for (i = 0; i < n; ++i) {
        seq[i] = (i == 0) ? 0 : (i == 1) ? 1 : seq[i - 1] + seq[i - 2];
    }

    /* Result CDR: encap header (CDR_LE) + u32 length + N int32. */
    uint8_t buf[8 + 4 * 16];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)n);
    for (i = 0; i < n; ++i) {
        write_u32_le(buf + 8 + 4 * i, (uint32_t)seq[i]);
    }
    size_t result_len = 8 + 4 * (size_t)n;

    int32_t rc = nros_cpp_action_server_complete_goal(
        self->storage, self->executor, (const uint8_t(*)[16])self->goal_id, buf, result_len);
    if (rc == 0) {
        printf("Goal succeeded\n");
    } else {
        printf("Failed to complete goal (rc=%d)\n", (int)rc);
    }
}

static nros_ret_t server_configure(const nros_cpp_node_t* node, void* executor,
                                   fib_server_t* self) {
    self->executor = executor;
    nros_cpp_qos_t qos = nros_c_qos_default();
    int32_t rc = nros_cpp_action_server_create(
        node, "/fibonacci", "example_interfaces/action/Fibonacci", "", qos, self->storage);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_action_server_register(self->storage, executor, "/fibonacci",
                                         "example_interfaces/action/Fibonacci", "",
                                         /*sched_context=*/0);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_action_server_set_callbacks(self->storage, on_goal, on_cancel, self);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    rc = nros_cpp_timer_create(executor, /*period_ms=*/200, on_tick, self, &timer_handle);
    if (rc == 0) {
        /* Readiness marker the e2e harness greps before sending a goal. */
        printf("Waiting for action goals\n");
    }
    return rc;
}

NROS_C_COMPONENT(fib_server_t, server_configure)
