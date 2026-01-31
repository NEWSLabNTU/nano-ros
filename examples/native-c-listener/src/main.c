/// @file main.c
/// @brief C listener example - subscribes to std_msgs/Int32 messages

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nano-ros modular includes (rclc-style)
#include <nano_ros/init.h>
#include <nano_ros/node.h>
#include <nano_ros/subscription.h>
#include <nano_ros/executor.h>

// ----------------------------------------------------------------------------
// std_msgs/Int32 message support (manual definition for this example)
// In a full setup, this would be auto-generated
// ----------------------------------------------------------------------------

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

// Initialize message to default values
static void std_msgs_Int32_init(std_msgs_Int32* msg) {
    msg->data = 0;
}

// Deserialize from CDR format
// CDR format for Int32: 4-byte header + 4-byte int32
static int32_t std_msgs_Int32_deserialize(std_msgs_Int32* msg, const uint8_t* buffer, size_t buffer_size) {
    if (buffer_size < 8) {
        return -1;
    }
    // Skip CDR header (4 bytes), read little-endian int32
    msg->data = (int32_t)(
        buffer[4] |
        ((uint32_t)buffer[5] << 8) |
        ((uint32_t)buffer[6] << 16) |
        ((uint32_t)buffer[7] << 24)
    );
    return 0;
}

// Message type info
static const nano_ros_message_type_t std_msgs_Int32_type = {
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

    std_msgs_Int32 msg;
    std_msgs_Int32_init(&msg);

    if (std_msgs_Int32_deserialize(&msg, data, len) == 0) {
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

    printf("nano-ros C Listener\n");
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

    // Initialize support context
    nano_ros_support_t support = nano_ros_support_get_zero_initialized();
    nano_ros_ret_t ret = nano_ros_support_init(&support, locator, domain_id);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }
    printf("Support initialized\n");

    // Create node
    nano_ros_node_t node = nano_ros_node_get_zero_initialized();
    ret = nano_ros_node_init(&node, &support, "c_listener", "/");
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Node created: %s\n", nano_ros_node_get_name(&node));

    // Create application context
    listener_context_t listener_ctx = {
        .message_count = 0,
    };

    // Create subscription
    nano_ros_subscription_t subscription = nano_ros_subscription_get_zero_initialized();
    ret = nano_ros_subscription_init(
        &subscription,
        &node,
        &std_msgs_Int32_type,
        "/chatter",
        subscription_callback,
        &listener_ctx
    );
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize subscription: %d\n", ret);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Subscription created for topic: %s\n", nano_ros_subscription_get_topic_name(&subscription));

    // Create executor
    nano_ros_executor_t executor = nano_ros_executor_get_zero_initialized();
    ret = nano_ros_executor_init(&executor, &support, 4);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nano_ros_subscription_fini(&subscription);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    g_executor = &executor;

    // Add subscription to executor
    ret = nano_ros_executor_add_subscription(&executor, &subscription, NANO_ROS_EXECUTOR_ON_NEW_DATA);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to add subscription to executor: %d\n", ret);
        nano_ros_executor_fini(&executor);
        nano_ros_subscription_fini(&subscription);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Executor created with %d handle(s)\n", nano_ros_executor_get_handle_count(&executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    ret = nano_ros_executor_spin_period(&executor, 100000000ULL);
    if (ret != NANO_ROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total messages received: %d\n", listener_ctx.message_count);
    nano_ros_executor_fini(&executor);
    nano_ros_subscription_fini(&subscription);
    nano_ros_node_fini(&node);
    nano_ros_support_fini(&support);

    printf("Goodbye!\n");
    return 0;
}
