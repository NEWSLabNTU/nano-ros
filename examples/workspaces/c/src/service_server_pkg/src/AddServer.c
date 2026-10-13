/// @file AddServer.c
/// @brief pure-C workspace — A1 services: the AddTwoInts service SERVER, typed component
/// (RFC-0043 / phase-257). The C projection of the Rust `service_server_pkg`.
///
/// `add_server_configure` creates a service server on `/add_two_ints` with the C API and the
/// generated typed handler (`example_interfaces__srv__AddTwoInts__{service_init,
/// service_handler_init,executor_add_service}`): the request arrives decoded and the handler
/// fills `sum = a + b`. Wired through the component factory the typed C Entry drives
/// (`nros_board_native_run_components`), not a standalone `main`.
///
/// Cross-process only (issue 0096): an in-process (same-executor) server+client can't talk, so
/// the server runs as its OWN entry (`native_service_server_entry`) and the client as another.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
// (nros_find_interfaces(LANGUAGE C) → example_interfaces__nano_ros_c).
#include "example_interfaces.h"

typedef struct {
    rcl_service_t service;
    example_interfaces__srv__AddTwoInts__service_handler_t handler;
    int request_count;
} add_server_t;

/// Typed handler: the request arrives decoded, the response is ours to fill.
/// A request that fails to decode never reaches here — the generated
/// trampoline refuses it and reports why (issue 0737).
static void on_request(const example_interfaces__srv__AddTwoInts_Request* req,
                       example_interfaces__srv__AddTwoInts_Response* resp, void* ctx) {
    add_server_t* self = (add_server_t*)ctx;
    self->request_count++;
    resp->sum = req->a + req->b;
    printf("[service_server_pkg] %lld + %lld = %lld\n", (long long)req->a, (long long)req->b,
           (long long)resp->sum);
}

static rcl_ret_t add_server_configure(rcl_node_t* node, rclc_executor_t* executor,
                                      add_server_t* self) {
    /* Line-buffer stdout so each line flushes when piped to the test harness. */
    setvbuf(stdout, NULL, _IOLBF, 0);
    self->request_count = 0;
    self->service = rcl_get_zero_initialized_service();
    rcl_ret_t rc =
        example_interfaces__srv__AddTwoInts__service_init(&self->service, node, "/add_two_ints");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc =
        example_interfaces__srv__AddTwoInts__service_handler_init(&self->handler, on_request, self);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    rc = example_interfaces__srv__AddTwoInts__executor_add_service(executor, &self->service,
                                                                   &self->handler);
    if (rc == RCL_RET_OK) {
        printf("[service_server_pkg] add_two_ints server ready\n");
    }
    return rc;
}

RCLC_C_COMPONENT(add_server_t, add_server_configure)
