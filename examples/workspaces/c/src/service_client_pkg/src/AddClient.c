/// @file AddClient.c
/// @brief pure-C workspace — A1 services: the AddTwoInts service CLIENT, typed component
/// (RFC-0043 / phase-257). The C projection of the Rust `service_client_pkg`.
///
/// The C component service client polls with the generated typed helpers
/// (`example_interfaces__srv__AddTwoInts__client_{send_request,take_response}`), not a blocking
/// call — a component callback must never block the executor. A 1 Hz timer drives the loop: poll
/// for the in-flight reply, on success print + republish the server-computed sum on `/sum`, then
/// send the next request. A wait counter re-sends if a reply never arrives (the first request(s)
/// can be dropped before the server is discovered). `a` runs 0,1,2,… with `b = 1`, so the sums are
/// 1,2,3,… (the server computes `a + b`) — proving the cross-process round-trip when the client
/// prints them.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

#include "example_interfaces.h"
#include "std_msgs.h"

typedef struct {
    rcl_client_t client;
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int64_t a;
    bool in_flight; /* a request is awaiting its reply */
    int waits;      /* ticks waited for the current reply (resend guard) */
} add_client_t;

static void send_next(add_client_t* self) {
    example_interfaces__srv__AddTwoInts_Request req = {.a = self->a, .b = 1};
    uint8_t scratch[64];
    if (example_interfaces__srv__AddTwoInts__client_send_request(&self->client, &req, scratch,
                                                                 sizeof(scratch)) == RCL_RET_OK) {
        self->in_flight = true;
        self->waits = 0;
    }
    self->a++;
}

static void on_tick(rcl_timer_t* timer, void* ctx) {
    (void)timer;
    add_client_t* self = (add_client_t*)ctx;

    if (self->in_flight) {
        example_interfaces__srv__AddTwoInts_Response r;
        uint8_t scratch[64];
        rcl_ret_t rc = example_interfaces__srv__AddTwoInts__client_take_response(
            &self->client, &r, scratch, sizeof(scratch));
        if (rc == RCL_RET_OK) {
            self->in_flight = false;
            printf("[service_client_pkg] sum: %lld\n", (long long)r.sum);
            /* republish the result on /sum */
            std_msgs__msg__Int32 sum_msg = {.data = (int32_t)r.sum};
            (void)rcl_publish(&self->pub, &sum_msg, NULL);
        } else if (++self->waits > 6) {
            /* No reply (request dropped before discovery) — resend. */
            self->in_flight = false;
        }
    }

    if (!self->in_flight) {
        send_next(self);
    }
}

static rcl_ret_t add_client_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      add_client_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->a = 0;
    self->in_flight = false;
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
    self->pub = rcl_get_zero_initialized_publisher();
    rc = rclc_publisher_init_default(&self->pub, node,
                                     ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/sum");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_with_context(&self->timer, 500ULL * 1000000ULL, on_tick, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(add_client_t, add_client_configure)
