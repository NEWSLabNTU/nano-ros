/// @file main.c
/// @brief C listener example - subscribes to std_msgs/Int32 messages

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>
#include <nros/executor.h>

// Generated message bindings
#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int message_count;
} listener_context_t;

// Static allocation — all nros structs live in .bss, not on the stack
static struct {
    nano_ros_support_t support;
    nros_node_t node;
    listener_context_t listener_ctx;
    nano_ros_subscription_t subscription;
    nano_ros_executor_t executor;
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
// Subscription callback - process received message
// ----------------------------------------------------------------------------

static void subscription_callback(const uint8_t* data, size_t len, void* context) {
    listener_context_t* ctx = (listener_context_t*)context;

    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    if (std_msgs_msg_int32_deserialize(&msg, data, len) == 0) {
        ctx->message_count++;
        printf("Received [%d]: %d\n", ctx->message_count, msg.data);
    } else {
        fprintf(stderr, "Failed to deserialize message (len=%zu)\n", len);
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C Listener\n");
    printf("===================\n");

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

    // Zero-initialize all static state (avoids return-by-value temporaries on stack)
    memset(&app, 0, sizeof(app));

    // Initialize support context
    nano_ros_ret_t ret = nano_ros_support_init(&app.support, locator, domain_id);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }
    printf("Support initialized\n");

    // Create node
    ret = nros_node_init(&app.node, &app.support, "c_listener", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    // Create application context
    app.listener_ctx = (listener_context_t){
        .message_count = 0,
    };

    // Create subscription using generated type support
    ret = nano_ros_subscription_init(
        &app.subscription,
        &app.node,
        std_msgs_msg_int32_get_type_support(),
        "/chatter",
        subscription_callback,
        &app.listener_ctx
    );
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize subscription: %d\n", ret);
        nros_node_fini(&app.node);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    printf("Subscription created for topic: %s\n", nano_ros_subscription_get_topic_name(&app.subscription));

    // Create executor
    ret = nano_ros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nano_ros_subscription_fini(&app.subscription);
        nros_node_fini(&app.node);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    g_executor = &app.executor;

    // Add subscription to executor
    ret = nano_ros_executor_add_subscription(&app.executor, &app.subscription, NROS_EXECUTOR_ON_NEW_DATA);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to add subscription to executor: %d\n", ret);
        nano_ros_executor_fini(&app.executor);
        nano_ros_subscription_fini(&app.subscription);
        nros_node_fini(&app.node);
        nano_ros_support_fini(&app.support);
        return 1;
    }
    printf("Executor created with %d handle(s)\n", nano_ros_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    ret = nano_ros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total messages received: %d\n", app.listener_ctx.message_count);
    nano_ros_executor_fini(&app.executor);
    nano_ros_subscription_fini(&app.subscription);
    nros_node_fini(&app.node);
    nano_ros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}
