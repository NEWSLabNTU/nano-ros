/// @file main.c
/// @brief C service server example - AddTwoInts service using executor

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>
#include <nros/executor.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int request_count;
} server_context_t;

// Static allocation
static struct {
    nano_ros_support_t support;
    nros_node_t node;
    nano_ros_service_t service;
    nano_ros_executor_t executor;
    server_context_t ctx;
} app;

static volatile sig_atomic_t g_running = 1;
static nano_ros_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nano_ros_executor_stop(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Service callback - handle AddTwoInts request
// ----------------------------------------------------------------------------

static bool service_callback(const uint8_t* request_data,
                             size_t request_len,
                             uint8_t* response_data,
                             size_t response_capacity,
                             size_t* response_len,
                             void* context) {
    server_context_t* ctx = (server_context_t*)context;

    // Deserialize request using generated function
    example_interfaces_srv_add_two_ints_request request;
    if (example_interfaces_srv_add_two_ints_request_deserialize(
            &request, request_data, request_len) != 0) {
        fprintf(stderr, "Failed to deserialize request\n");
        return false;
    }

    ctx->request_count++;

    // Compute response
    example_interfaces_srv_add_two_ints_response response;
    example_interfaces_srv_add_two_ints_response_init(&response);
    response.sum = request.a + request.b;

    printf("Request [%d]: %lld + %lld = %lld\n",
           ctx->request_count,
           (long long)request.a,
           (long long)request.b,
           (long long)response.sum);

    // Serialize response using generated function
    int32_t len = example_interfaces_srv_add_two_ints_response_serialize(
        &response, response_data, response_capacity);
    if (len < 0) {
        fprintf(stderr, "Failed to serialize response\n");
        return false;
    }

    *response_len = (size_t)len;
    return true;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C Service Server (AddTwoInts)\n");
    printf("=====================================\n");

    // Get configuration from environment
    const char* locator = getenv("ZENOH_LOCATOR");
    if (!locator) {
        locator = "tcp/127.0.0.1:7447";
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Locator: %s\n", locator);
    printf("Domain ID: %d\n", domain_id);

    // Zero-initialize all static state
    memset(&app, 0, sizeof(app));

    // Build type info using generated type name/hash
    nano_ros_message_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
        .serialized_size_max = 256,
    };

    // Initialize support context
    nano_ros_ret_t ret = nano_ros_support_init(&app.support, locator, domain_id);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }
    printf("Support initialized\n");

    // Create node
    ret = nros_node_init(&app.node, &app.support, "c_service_server", "/");
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    // Create service server
    ret = nano_ros_service_init(
        &app.service,
        &app.node,
        &add_two_ints_type,
        "/add_two_ints",
        service_callback,
        &app.ctx
    );
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize service: %d\n", ret);
        nros_node_fini(&app.node);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    printf("Service created: %s\n", nano_ros_service_get_service_name(&app.service));

    // Create executor
    ret = nano_ros_executor_init(&app.executor, &app.support, 4);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nano_ros_service_fini(&app.service);
        nros_node_fini(&app.node);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    g_executor = &app.executor;

    // Add service to executor
    ret = nano_ros_executor_add_service(&app.executor, &app.service);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to add service to executor: %d\n", ret);
        nano_ros_executor_fini(&app.executor);
        nano_ros_service_fini(&app.service);
        nros_node_fini(&app.node);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    printf("Executor created with %d handle(s)\n",
           nano_ros_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for service requests (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    ret = nano_ros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NANO_ROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total requests handled: %d\n", app.ctx.request_count);
    nano_ros_executor_fini(&app.executor);
    nano_ros_service_fini(&app.service);
    nros_node_fini(&app.node);
    nano_ros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}
