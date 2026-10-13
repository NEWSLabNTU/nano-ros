/// @file AddTwoIntsServer.c
/// @brief Zephyr C AddTwoInts service server — typed component.
///
/// `server_configure` creates a service on `/add_two_ints` with the C API and
/// the generated typed handler (`example_interfaces__srv__AddTwoInts__*`): the
/// request arrives decoded (int64 a, b) and `on_request` fills the reply
/// (int64 sum). `RCLC_C_COMPONENT` emits the C-ABI factory + configure seam the
/// Zephyr typed Entry carrier (the shared entry pack) drives. No interpreter
/// synthesis.

#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>

#include <example_interfaces/srv/add_two_ints.h>

typedef struct {
    rcl_service_t service;
    example_interfaces__srv__AddTwoInts__service_handler_t handler;
    int served;
} add_server_t;

static void on_request(const example_interfaces__srv__AddTwoInts_Request* req,
                       example_interfaces__srv__AddTwoInts_Response* resp, void* ctx) {
    add_server_t* self = (add_server_t*)ctx;
    resp->sum = req->a + req->b;
    self->served++;
    printf("Incoming request\na: %lld b: %lld\n", (long long)req->a, (long long)req->b);
}

static rcl_ret_t server_configure(rcl_node_t* node, rclc_executor_t* executor, add_server_t* self) {
    setvbuf(stdout, NULL, _IONBF, 0);
    self->served = 0;
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
        /* Readiness marker the e2e harness greps before driving the client. */
        printf("Waiting for service requests\n");
    }
    return rc;
}

RCLC_C_COMPONENT(add_server_t, server_configure)
