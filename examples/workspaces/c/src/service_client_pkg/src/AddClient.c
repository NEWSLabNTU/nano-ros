/// @file AddClient.c
/// @brief pure-C workspace — the AddTwoInts service CLIENT, a typed component
/// (RFC-0043 / phase-257). The C projection of the Rust `service_client_pkg`.
///
/// The ROS 2 tutorial client (demo_nodes_cpp `add_two_ints_client`): ONE request, a = 2 and
/// b = 3, and one line, `Result of add_two_ints: 5`. phase-535 made this the ONE C client: the
/// standalone example pairs and the workspace images run the same program (RFC-0122 option B).
///
/// A component callback must never block the executor, so the call is not the tutorial's
/// blocking one: a 500 ms timer sends the request with the generated typed helpers
/// (`example_interfaces__srv__AddTwoInts__client_{send_request,take_response}`) and polls for
/// the reply. A wait counter re-sends if a reply never arrives (the first request can be
/// dropped before the server is discovered). After the reply the timer only idles.

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
    bool in_flight; /* a request is awaiting its reply */
    bool done;      /* the reply arrived and was printed */
    int waits;      /* ticks waited for the current reply (resend guard) */
} add_client_t;

static void send_request(add_client_t* self) {
    example_interfaces__srv__AddTwoInts_Request req = {.a = ADD_CLIENT_A, .b = ADD_CLIENT_B};
    uint8_t scratch[64];
    if (example_interfaces__srv__AddTwoInts__client_send_request(&self->client, &req, scratch,
                                                                 sizeof(scratch)) == RCL_RET_OK) {
        self->in_flight = true;
        self->waits = 0;
    }
}

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    add_client_t* self = RCLC_CONTAINER_OF(timer, add_client_t, timer);
    (void)last_call_time;
    if (self->done) {
        return;
    }

    if (self->in_flight) {
        example_interfaces__srv__AddTwoInts_Response r;
        uint8_t scratch[64];
        rcl_ret_t rc = example_interfaces__srv__AddTwoInts__client_take_response(
            &self->client, &r, scratch, sizeof(scratch));
        if (rc == RCL_RET_OK) {
            self->in_flight = false;
            self->done = true;
            printf("Result of add_two_ints: %lld\n", (long long)r.sum);
            return;
        }
        if (++self->waits > 6) {
            /* No reply (request dropped before discovery) — resend. */
            self->in_flight = false;
        }
    }

    if (!self->in_flight) {
        send_request(self);
    }
}

static rcl_ret_t add_client_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      add_client_t* self) {
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    self->in_flight = false;
    self->done = false;
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
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 500ULL * 1000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(add_client_t, add_client_configure)
