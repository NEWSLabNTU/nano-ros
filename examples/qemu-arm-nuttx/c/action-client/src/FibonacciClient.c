/// @file FibonacciClient.c
/// @brief NuttX C Fibonacci action client — typed poll component (240.5).
///
/// `client_configure` creates an action client + a timer that drives a poll
/// state machine: send goal → poll acceptance → fetch result → print it.
/// (Poll model — clients move to callbacks when RFC-0041's C/C++ wave lands.)

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t storage[NROS_C_ACTION_CLIENT_STORAGE_SIZE];
    void* executor;
    int phase; /* 0 send, 1 await-accept, 2 get-result, 3 done */
    int32_t order;
    uint8_t goal_id[16];
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

static void on_tick(void* ctx) {
    fib_client_t* self = (fib_client_t*)ctx;
    if (self->phase == 0) {
        uint8_t goal[8];
        goal[0] = 0x00;
        goal[1] = 0x01;
        goal[2] = 0x00;
        goal[3] = 0x00;
        write_u32_le(goal + 4, (uint32_t)self->order);
        if (nros_cpp_action_client_send_goal(self->storage, goal, sizeof(goal), &self->goal_id) ==
            0) {
            printf("Goal sent: order=%d\n", (int)self->order);
            self->phase = 1;
        }
    } else if (self->phase == 1) {
        uint8_t buf[17];
        size_t len = 0;
        if (nros_cpp_action_client_try_recv_goal_response(self->storage, buf, sizeof(buf), &len) ==
                0 &&
            len >= 17) {
            if (buf[16] != 0) {
                printf("Goal accepted by server\n");
                self->phase = 2;
            } else {
                printf("Goal rejected by server\n");
                self->phase = 3;
            }
        }
    } else if (self->phase == 2) {
        uint8_t res[256];
        size_t len = 0;
        if (nros_cpp_action_client_get_result(self->storage, self->executor,
                                              (const uint8_t(*)[16])self->goal_id, res, sizeof(res),
                                              &len) == 0 &&
            len >= 8) {
            uint32_t count = read_u32_le(res + 4);
            printf("Result received: %u terms\n", (unsigned)count);
            self->phase = 3;
        }
    }
}

static nros_ret_t client_configure(const nros_cpp_node_t* node, void* executor,
                                   fib_client_t* self) {
    self->executor = executor;
    self->order = 5;
    int32_t rc =
        nros_cpp_action_client_create(node, "/fibonacci", "example_interfaces/action/Fibonacci", "",
                                      nros_c_qos_default(), self->storage);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    rc = nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
    if (rc == 0) {
        printf("Sending goal\n");
    }
    return rc;
}

NROS_C_COMPONENT(fib_client_t, client_configure)
