/// @file main.c
/// @brief NuttX C service server example - AddTwoInts service

#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>
#include <nros/executor.h>

#include "example_interfaces.h"

// NuttX embedded config — matches board crate defaults (server = 192.0.3.10)
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

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

static bool service_callback(const uint8_t* request_data,
                             size_t request_len,
                             uint8_t* response_data,
                             size_t response_capacity,
                             size_t* response_len,
                             void* context) {
    server_context_t* ctx = (server_context_t*)context;

    example_interfaces_srv_add_two_ints_request request;
    if (example_interfaces_srv_add_two_ints_request_deserialize(
            &request, request_data, request_len) != 0) {
        fprintf(stderr, "Failed to deserialize request\n");
        return false;
    }

    ctx->request_count++;

    example_interfaces_srv_add_two_ints_response response;
    example_interfaces_srv_add_two_ints_response_init(&response);
    response.sum = request.a + request.b;

    printf("Request [%d]: %lld + %lld = %lld\n",
           ctx->request_count,
           (long long)request.a,
           (long long)request.b,
           (long long)response.sum);

    int32_t len = example_interfaces_srv_add_two_ints_response_serialize(
        &response, response_data, response_capacity);
    if (len < 0) {
        fprintf(stderr, "Failed to serialize response\n");
        return false;
    }

    *response_len = (size_t)len;
    return true;
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros NuttX C Service Server (AddTwoInts)\n");
    printf("Locator: %s\n", APP_ZENOH_LOCATOR);

    memset(&app, 0, sizeof(app));

    nros_message_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
        .serialized_size_max = 256,
    };

    nros_ret_t ret = nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }

    ret = nros_node_init(&app.node, &app.support, "nuttx_c_service_server", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }

    ret = nros_service_init(
        &app.service, &app.node, &add_two_ints_type,
        "/add_two_ints", service_callback, &app.ctx);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize service: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nros_service_fini(&app.service);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    nros_executor_add_service(&app.executor, &app.service);

    printf("Waiting for requests...\n\n");
    nros_executor_spin_period(&app.executor, 100000000ULL);

    nros_executor_fini(&app.executor);
    nros_service_fini(&app.service);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return 0;
}
