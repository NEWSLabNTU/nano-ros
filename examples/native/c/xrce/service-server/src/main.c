/// @file main.c
/// @brief C service server example (XRCE-DDS) - AddTwoInts service using executor

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>

#include "example_interfaces.h"

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

static volatile sig_atomic_t g_running = 1;
static nros_executor_t* g_executor = NULL;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nros_executor_stop(g_executor);
    }
}

static bool service_callback(const uint8_t* request_data, size_t request_len,
                             uint8_t* response_data, size_t response_capacity, size_t* response_len,
                             void* context) {
    server_context_t* ctx = (server_context_t*)context;

    example_interfaces_srv_add_two_ints_request request;
    if (example_interfaces_srv_add_two_ints_request_deserialize(&request, request_data,
                                                                request_len) != 0) {
        fprintf(stderr, "Failed to deserialize request\n");
        return false;
    }

    ctx->request_count++;

    example_interfaces_srv_add_two_ints_response response;
    example_interfaces_srv_add_two_ints_response_init(&response);
    response.sum = request.a + request.b;

    printf("Request [%d]: %lld + %lld = %lld\n", ctx->request_count, (long long)request.a,
           (long long)request.b, (long long)response.sum);

    int32_t len = example_interfaces_srv_add_two_ints_response_serialize(&response, response_data,
                                                                         response_capacity);
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

    printf("nros C Service Server - XRCE (AddTwoInts)\n");
    printf("============================================\n");

    const char* agent = getenv("XRCE_AGENT_ADDR");
    if (!agent) {
        agent = "127.0.0.1:2019";
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Agent: %s\n", agent);
    printf("Domain ID: %d\n", domain_id);

    memset(&app, 0, sizeof(app));

    nros_service_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    NROS_CHECK_RET(nros_support_init(&app.support, agent, domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_xrce_service_server", "/"), 1);
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    NROS_CHECK_RET(nros_service_init(&app.service, &app.node, &add_two_ints_type,
                                     "/add_two_ints", service_callback, &app.ctx), 1);
    printf("Service created: %s\n", nros_service_get_service_name(&app.service));

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    NROS_CHECK_RET(nros_executor_add_service(&app.executor, &app.service), 1);

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for service requests (Ctrl+C to exit)...\n\n");

    nros_ret_t ret = nros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    printf("\nShutting down...\n");
    printf("Total requests handled: %d\n", app.ctx.request_count);
    nros_executor_fini(&app.executor);
    nros_service_fini(&app.service);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}
