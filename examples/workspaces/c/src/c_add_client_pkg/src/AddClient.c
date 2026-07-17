/// @file AddClient.c
/// @brief pure-C workspace — A1 services: the AddTwoInts service CLIENT, typed component
/// (RFC-0043 / phase-257). The C projection of the Rust `service_client_pkg`.
///
/// The C component service client is POLL-model (`nros_cpp_service_client_{create,
/// send_request,try_recv_reply}`), not blocking — a component callback must never block the
/// executor. A 1 Hz timer drives the loop: poll for the in-flight reply, on success print +
/// republish the server-computed sum on `/sum`, then send the next request. A wait counter
/// re-sends if a reply never arrives (the first request(s) can be dropped before the server is
/// discovered). `a` runs 0,1,2,… with `b = 1`, so the sums are 1,2,3,… (the server computes
/// `a + b`) — proving the cross-process round-trip when the client prints them.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

#include "example_interfaces.h"
#include "std_msgs.h"

typedef struct {
    _Alignas(8) uint8_t client[NROS_C_SERVICE_CLIENT_STORAGE_SIZE];
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int64_t a;
    bool in_flight; /* a request is awaiting its reply */
    int waits;      /* ticks waited for the current reply (resend guard) */
} add_client_t;

static void send_next(add_client_t* self) {
    example_interfaces_srv_add_two_ints_request req;
    example_interfaces_srv_add_two_ints_request_init(&req);
    req.a = self->a;
    req.b = 1;
    uint8_t buf[64];
    size_t n = 0;
    int32_t n_rc =
        example_interfaces_srv_add_two_ints_request_serialize(&req, buf, sizeof(buf), &n);
    if (n_rc == 0 && nros_cpp_service_client_send_request(self->client, buf, n) == 0) {
        self->in_flight = true;
        self->waits = 0;
    }
    self->a++;
}

static void on_tick(void* ctx) {
    add_client_t* self = (add_client_t*)ctx;

    if (self->in_flight) {
        uint8_t resp[64];
        size_t rlen = 0;
        if (nros_cpp_service_client_try_recv_reply(self->client, resp, sizeof(resp), &rlen) == 0 &&
            rlen > 0) {
            example_interfaces_srv_add_two_ints_response r;
            if (example_interfaces_srv_add_two_ints_response_deserialize(&r, resp, rlen) == 0) {
                self->in_flight = false;
                printf("[c_add_client_pkg] sum: %lld\n", (long long)r.sum);
                /* republish the result on /sum (std_msgs/Int32, generated serializer) */
                std_msgs_msg_int32 sum_msg;
                std_msgs_msg_int32_init(&sum_msg);
                sum_msg.data = (int32_t)r.sum;
                uint8_t pbuf[16];
                size_t plen = 0;
                if (std_msgs_msg_int32_serialize(&sum_msg, pbuf, sizeof(pbuf), &plen) == 0) {
                    (void)nros_cpp_publish_raw(self->pub, pbuf, plen);
                }
            }
        } else if (++self->waits > 6) {
            /* No reply (request dropped before discovery) — resend. */
            self->in_flight = false;
        }
    }

    if (!self->in_flight) {
        send_next(self);
    }
}

static nros_ret_t add_client_configure(const nros_cpp_node_t* node, void* executor,
                                       add_client_t* self) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->a = 0;
    self->in_flight = false;
    self->waits = 0;

    int32_t rc = nros_cpp_service_client_create(
        node, "/add_two_ints", example_interfaces_srv_add_two_ints_get_type_name(),
        example_interfaces_srv_add_two_ints_get_type_hash(), nros_c_qos_default(), self->client);
    if (rc != 0) {
        return rc;
    }
    rc = nros_cpp_publisher_create(node, "/sum", std_msgs_msg_int32_get_type_name(),
                                   std_msgs_msg_int32_get_type_hash(), nros_c_qos_default(),
                                   self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(add_client_t, add_client_configure)
