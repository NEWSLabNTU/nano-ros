/// @file main.c
/// @brief FreeRTOS C service server — AddTwoInts on /add_two_ints

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>

#include <nros/app_config.h>
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int request_count;
} server_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_service_t service;
    nros_executor_t executor;
    server_context_t ctx;
} app;

// ----------------------------------------------------------------------------
// Service callback
// ----------------------------------------------------------------------------

static bool service_callback(const uint8_t *request_data, size_t request_len,
                             uint8_t *response_data, size_t response_capacity,
                             size_t *response_len, void *context) {
    server_context_t *ctx = (server_context_t *)context;

    example_interfaces_srv_add_two_ints_request request;
    if (example_interfaces_srv_add_two_ints_request_deserialize(
            &request, request_data, request_len) != 0) {
        printf("Failed to deserialize request\n");
        return false;
    }

    ctx->request_count++;

    example_interfaces_srv_add_two_ints_response response;
    example_interfaces_srv_add_two_ints_response_init(&response);
    response.sum = request.a + request.b;

    printf("Request [%d]: %lld + %lld = %lld\n", ctx->request_count,
           (long long)request.a, (long long)request.b, (long long)response.sum);

    int32_t ser_len = example_interfaces_srv_add_two_ints_response_serialize(
            &response, response_data, response_capacity);
    if (ser_len < 0) {
        printf("Failed to serialize response\n");
        return false;
    }

    *response_len = (size_t)ser_len;
    return true;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C Service Server (FreeRTOS)\n");

    memset(&app, 0, sizeof(app));

    nros_service_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    NROS_CHECK_RET(nros_support_init(&app.support, NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_service_server", "/"), 1);
    NROS_CHECK_RET(nros_service_init(&app.service, &app.node, &add_two_ints_type,
                                 "/add_two_ints", service_callback, &app.ctx), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_service(&app.executor, &app.service), 1);

    printf("Service server ready on /add_two_ints\n");
    printf("Waiting for requests...\n");

    for (int i = 0; i < 50000; i++) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
    }

    printf("Server shutting down.\n");

    nros_executor_fini(&app.executor);
    nros_service_fini(&app.service);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}

NROS_APP_MAIN_REGISTER_VOID()
