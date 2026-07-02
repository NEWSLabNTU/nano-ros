/// @file ServiceClient.c
/// @brief QEMU RISC-V ThreadX C AddTwoInts service client — typed poll component (RFC-0043).
///
/// `client_configure` creates a service client + a timer that polls: the first
/// tick sends ONE fixed request (2, 3); later ticks poll the reply and print
/// the sum, then the client goes quiet. (Poll model — clients move to
/// callbacks when RFC-0041's C/C++ wave lands.)

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t storage[NROS_C_SERVICE_CLIENT_STORAGE_SIZE];
    void* executor;
    int64_t a;
    int64_t b;
    int awaiting;
    int done;
} add_client_t;

static int64_t read_i64_le(const uint8_t* p) {
    uint64_t v = 0;
    int i;
    for (i = 0; i < 8; ++i) {
        v |= (uint64_t)p[i] << (8 * i);
    }
    return (int64_t)v;
}

static void write_i64_le(uint8_t* p, int64_t x) {
    uint64_t v = (uint64_t)x;
    int i;
    for (i = 0; i < 8; ++i) {
        p[i] = (uint8_t)(v >> (8 * i));
    }
}

static void on_tick(void* ctx) {
    add_client_t* self = (add_client_t*)ctx;
    if (self->done) {
        return;
    }
    if (!self->awaiting) {
        uint8_t req[20];
        req[0] = 0x00;
        req[1] = 0x01;
        req[2] = 0x00;
        req[3] = 0x00;
        write_i64_le(req + 4, self->a);
        write_i64_le(req + 12, self->b);
        if (nros_cpp_service_client_send_request(self->storage, req, sizeof(req)) == 0) {
            self->awaiting = 1;
        }
        return;
    }
    uint8_t resp[64];
    size_t len = 0;
    if (nros_cpp_service_client_try_recv_reply(self->storage, resp, sizeof(resp), &len) == 0 &&
        len >= 12) {
        int64_t sum = read_i64_le(resp + 4);
        printf("Result of add_two_ints: %lld\n", (long long)sum);
        self->done = 1;
    }
}

static nros_ret_t client_configure(const nros_cpp_node_t* node, void* executor,
                                   add_client_t* self) {
    self->executor = executor;
    self->a = 2;
    self->b = 3;
    int32_t rc =
        nros_cpp_service_client_create(node, "/add_two_ints", "example_interfaces/srv/AddTwoInts",
                                       "", nros_c_qos_default(), self->storage);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    rc = nros_cpp_timer_create(executor, /*period_ms=*/1000, on_tick, self, &timer_handle);
    if (rc == 0) {
        printf("Sending request\n");
    }
    return rc;
}

NROS_C_COMPONENT(add_client_t, client_configure)
