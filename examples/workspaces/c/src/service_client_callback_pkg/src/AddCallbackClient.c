/// @file AddCallbackClient.c
/// @brief pure-C workspace — the AddTwoInts service client, **callback** variant (RFC-0041).
///
/// The same tutorial request as `service_client_pkg` (2 + 3, one line
/// `Result of add_two_ints: 5`), but the reply is NOT polled: it lands in a typed response
/// callback that `rclc_executor_spin_some` dispatches — the C analogue of rclcpp
/// `async_send_request(req, cb)`. phase-535 W6 (RFC-0122 D2) made the old
/// `examples/linux/c/service-client-callback` leaf this node package.
///
/// The callback is TYPED, which is rclc's shape (phase-417 W5.e):
///
///     rclc: void (*rclc_client_callback_t)(const void *response)
///     ours: void (*..._response_fn_t)(const ..._response *response, void *context)
///
/// The response storage is the CALLER's (the handler below), so nothing on the reply path
/// allocates, and a reply whose CDR does not decode never reaches the callback.
///
/// A component callback must never block the executor, so a 500 ms timer sends the request and
/// re-sends while no reply has arrived (the first request can be dropped before the server is
/// discovered). After the reply the timer only idles.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

#include "example_interfaces.h"

/// The tutorial's request: 2 + 3.
#define ADD_CLIENT_A 2
#define ADD_CLIENT_B 3

typedef struct {
    rcl_client_t client;
    rcl_timer_t timer;
    /// Caller-owned response storage + the typed callback; outlives the client.
    example_interfaces__srv__AddTwoInts__client_handler_t handler;
    int replies; /* bumped by the response callback */
    int waits;   /* ticks waited for the current reply (resend guard) */
} add_callback_client_t;

static void on_response(const example_interfaces__srv__AddTwoInts_Response* response,
                        void* context) {
    add_callback_client_t* self = (add_callback_client_t*)context;
    self->replies++;
    printf("Result of add_two_ints: %lld\n", (long long)response->sum);
}

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    add_callback_client_t* self = RCLC_CONTAINER_OF(timer, add_callback_client_t, timer);
    (void)last_call_time;
    if (self->replies > 0) {
        return;
    }
    if (self->waits > 0 && self->waits <= 6) {
        self->waits++;
        return;
    }
    example_interfaces__srv__AddTwoInts_Request req = {.a = ADD_CLIENT_A, .b = ADD_CLIENT_B};
    uint8_t scratch[64];
    if (example_interfaces__srv__AddTwoInts__client_send_request(&self->client, &req, scratch,
                                                                 sizeof(scratch)) == RCL_RET_OK) {
        self->waits = 1;
    }
}

static rcl_ret_t add_callback_client_configure(rcl_node_t* node, rclc_executor_t* executor,
                                               add_callback_client_t* self) {
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    self->replies = 0;
    self->waits = 0;

    self->client = rcl_get_zero_initialized_client();
    rcl_ret_t rc = rclc_client_init_default(
        &self->client, node, ROSIDL_GET_SRV_TYPE_SUPPORT(example_interfaces, srv, AddTwoInts),
        "/add_two_ints");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = rclc_executor_attach_client(executor, &self->client);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    // Register the reply callback. It fires at spin, not via a poll.
    rc =
        example_interfaces__srv__AddTwoInts__client_handler_init(&self->handler, on_response, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = example_interfaces__srv__AddTwoInts__client_set_response_callback(&self->client,
                                                                           &self->handler);
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

RCLC_C_COMPONENT(add_callback_client_t, add_callback_client_configure)
