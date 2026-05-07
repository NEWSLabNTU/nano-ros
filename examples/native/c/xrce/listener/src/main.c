/// @file main.c
/// @brief C XRCE-DDS listener example - subscribes to std_msgs/Int32 messages
///
/// Build nros-c with XRCE features:
///   cargo build -p nros-c --release --features "rmw-xrce,xrce-udp,platform-posix,ros-humble"
///
/// Run with MicroXRCEAgent:
///   MicroXRCEAgent udp4 -p 2019
///   ./c_xrce_listener

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>

// ----------------------------------------------------------------------------
// std_msgs/Int32 message support (manual definition for this example)
// In a full setup, this would be auto-generated
// ----------------------------------------------------------------------------

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

// Deserialize from CDR format
// CDR format for Int32: 4-byte header + 4-byte int32
static int32_t std_msgs_Int32_deserialize(std_msgs_Int32* msg, const uint8_t* buffer,
                                          size_t buffer_size) {
    if (buffer_size < 8) {
        return -1;
    }
    // Skip CDR header (4 bytes), read little-endian int32
    msg->data = (int32_t)(buffer[4] | ((uint32_t)buffer[5] << 8) | ((uint32_t)buffer[6] << 16) |
                          ((uint32_t)buffer[7] << 24));
    return 0;
}

// Message type info
static const nros_message_type_t std_msgs_Int32_type = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_5bf22a2e7c2c8a4ca3f55054648f6d8c7c77cc0ae5695a1ff1df0b7ef8df1f09",
    .serialized_size_max = 8,
};

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int message_count;
} listener_context_t;

// Static allocation — all nros structs live in .bss, not on the stack
static struct {
    nros_support_t support;
    nros_node_t node;
    listener_context_t listener_ctx;
    nros_subscription_t subscription;
    nros_executor_t executor;
} app;

static volatile sig_atomic_t g_running = 1;
static nros_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nros_executor_stop(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Subscription callback - process received message
// ----------------------------------------------------------------------------

static void subscription_callback(const uint8_t* data, size_t len, void* context) {
    listener_context_t* ctx = (listener_context_t*)context;

    std_msgs_Int32 msg;
    msg.data = 0;

    if (std_msgs_Int32_deserialize(&msg, data, len) == 0) {
        ctx->message_count++;
        printf("Received: %d\n", msg.data);
    } else {
        fprintf(stderr, "Failed to deserialize message (len=%zu)\n", len);
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C XRCE-DDS Listener\n");
    printf("========================\n");

    // Get configuration from environment
    const char* agent_addr = getenv("XRCE_AGENT_ADDR");
    if (!agent_addr) {
        agent_addr = "127.0.0.1:2019";
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Agent: %s\n", agent_addr);
    printf("Domain ID: %d\n", domain_id);

    // Zero-initialize all static state
    memset(&app, 0, sizeof(app));

    NROS_CHECK_RET(nros_support_init(&app.support, agent_addr, domain_id), 1);
    printf("Support initialized\n");
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_xrce_listener", "/"), 1);
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    app.listener_ctx = (listener_context_t){.message_count = 0};

    NROS_CHECK_RET(nros_subscription_init(&app.subscription, &app.node, &std_msgs_Int32_type,
                                          "/chatter", subscription_callback, &app.listener_ctx),
                   1);
    printf("Subscription created for topic: %s\n",
           nros_subscription_get_topic_name(&app.subscription));

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    NROS_CHECK_RET(
        nros_executor_add_subscription(&app.executor, &app.subscription, NROS_EXECUTOR_ON_NEW_DATA),
        1);
    printf("Executor created with %d handle(s)\n", nros_executor_get_handle_count(&app.executor));

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    nros_ret_t ret = nros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total messages received: %d\n", app.listener_ctx.message_count);
    nros_executor_fini(&app.executor);
    nros_subscription_fini(&app.subscription);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
