/// @file ActionClient.c
/// @brief QEMU RISC-V ThreadX C Fibonacci action client — typed CALLBACK component (RFC-0041/0043).
///
/// `client_configure` registers goal-response/feedback/result callbacks via
/// set_callbacks + a poll timer that drains the GET-query replies each spin tick,
/// then sends one goal. Acceptance + result arrive in the callbacks.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t storage[NROS_C_ACTION_CLIENT_STORAGE_SIZE];
    int32_t order;
} fib_client_t;

static uint32_t read_u32_le(const uint8_t* p) {
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_goal_response(bool accepted, const uint8_t goal_id[16], void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;
    if (accepted) {
        printf("Goal accepted by server\n");
        nros_cpp_action_client_get_result_async(self->storage, (const uint8_t(*)[16])goal_id);
    } else {
        printf("Goal rejected by server\n");
    }
}

static void on_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) {
    (void)goal_id;
    (void)data;
    (void)len;
    (void)ctx;
}

static void on_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len,
                      void* ctx) {
    (void)goal_id;
    (void)ctx;
    uint32_t count = (len >= 8) ? read_u32_le(data + 4) : 0u;
    printf("Result (status=%d): %u terms\n", (int)status, (unsigned)count);
    printf("Action completed successfully\n");
}

static void on_poll(void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;
    nros_cpp_action_client_poll(self->storage); /* drain GET replies -> callbacks */
}

static nros_ret_t client_configure(const nros_cpp_node_t* node, void* executor,
                                   fib_client_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0); /* callbacks print on transitions only */
    self->order = 5;
    int32_t rc =
        nros_cpp_action_client_create(node, "/fibonacci", "example_interfaces/action/Fibonacci", "",
                                      nros_c_qos_default(), self->storage);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_action_client_set_callbacks(self->storage, on_goal_response, on_feedback,
                                              on_result, self);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    rc = nros_cpp_timer_create(executor, /*period_ms=*/20, on_poll, self, &timer_handle);
    if (rc != 0) {
        return rc;
    }
    /* Send one goal (async — acceptance arrives in on_goal_response). */
    uint8_t goal[8];
    goal[0] = 0x00;
    goal[1] = 0x01;
    goal[2] = 0x00;
    goal[3] = 0x00;
    write_u32_le(goal + 4, (uint32_t)self->order);
    uint8_t goal_id[16];
    nros_cpp_action_client_send_goal_async(self->storage, goal, sizeof(goal), &goal_id);
    printf("Goal sent: order=%d\n", (int)self->order);
    return 0;
}

NROS_C_COMPONENT(fib_client_t, client_configure)
