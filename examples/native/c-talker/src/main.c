/// @file main.c
/// @brief C talker example - publishes std_msgs/Int32 messages using timer

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nano-ros modular includes (rclc-style)
#include <nano_ros/init.h>
#include <nano_ros/node.h>
#include <nano_ros/publisher.h>
#include <nano_ros/timer.h>
#include <nano_ros/executor.h>
#include <nano_ros/clock.h>
#include <nano_ros/parameter.h>

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

// Serialize to CDR format
// CDR format for Int32: 4-byte header (0x00, 0x01, 0x00, 0x00) + 4-byte int32
static int32_t std_msgs_Int32_serialize(const std_msgs_Int32* msg, uint8_t* buffer, size_t buffer_size) {
    if (buffer_size < 8) {
        return -1;
    }
    // CDR header (big-endian format flag = 0x00, little-endian = 0x01)
    buffer[0] = 0x00;  // CDR encapsulation
    buffer[1] = 0x01;  // Little-endian
    buffer[2] = 0x00;  // Reserved
    buffer[3] = 0x00;  // Reserved
    // Little-endian int32
    buffer[4] = (uint8_t)(msg->data & 0xFF);
    buffer[5] = (uint8_t)((msg->data >> 8) & 0xFF);
    buffer[6] = (uint8_t)((msg->data >> 16) & 0xFF);
    buffer[7] = (uint8_t)((msg->data >> 24) & 0xFF);
    return 8;
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
    nano_ros_publisher_t* publisher;
    std_msgs_Int32 message;
    int count;
} talker_context_t;

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
// Timer callback - publish a message
// ----------------------------------------------------------------------------

static void timer_callback(struct nano_ros_timer_t* timer, void* context) {
    (void)timer;
    talker_context_t* ctx = (talker_context_t*)context;

    ctx->count++;
    ctx->message.data = ctx->count;

    uint8_t buffer[64];
    int32_t len = std_msgs_Int32_serialize(&ctx->message, buffer, sizeof(buffer));

    if (len > 0) {
        nano_ros_ret_t ret = nano_ros_publish_raw(ctx->publisher, buffer, (size_t)len);
        if (ret == NANO_ROS_RET_OK) {
            printf("Published: %d\n", ctx->message.data);
        } else {
            fprintf(stderr, "Publish failed: %d\n", ret);
        }
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nano-ros C Talker\n");
    printf("=================\n");

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

    // Demo: Initialize and use clock API
    nano_ros_clock_t clock = nano_ros_clock_get_zero_initialized();
    nano_ros_ret_t clock_ret = nano_ros_clock_init(&clock, NANO_ROS_CLOCK_SYSTEM_TIME);
    if (clock_ret == NANO_ROS_RET_OK) {
        nano_ros_time_t now;
        if (nano_ros_clock_get_now(&clock, &now) == NANO_ROS_RET_OK) {
            printf("System time: %d.%09u sec\n", now.sec, now.nanosec);
        }
        (void)nano_ros_clock_fini(&clock);
    }

    // Demo: Initialize and use parameter server
    nano_ros_parameter_t param_storage[8];  // Storage for up to 8 parameters
    nano_ros_param_server_t params = nano_ros_param_server_get_zero_initialized();
    if (nano_ros_param_server_init(&params, param_storage, 8) == NANO_ROS_RET_OK) {
        // Declare parameters with default values
        nano_ros_param_declare_bool(&params, "verbose", false);
        nano_ros_param_declare_integer(&params, "publish_rate_hz", 1);
        nano_ros_param_declare_double(&params, "scale_factor", 1.0);
        nano_ros_param_declare_string(&params, "topic_name", "/chatter");

        // Read back and display parameter values
        bool verbose = false;
        int64_t rate_hz = 0;
        double scale = 0.0;
        char topic[64] = {0};

        nano_ros_param_get_bool(&params, "verbose", &verbose);
        nano_ros_param_get_integer(&params, "publish_rate_hz", &rate_hz);
        nano_ros_param_get_double(&params, "scale_factor", &scale);
        nano_ros_param_get_string(&params, "topic_name", topic, sizeof(topic));

        printf("Parameters: verbose=%s, rate=%lld Hz, scale=%.2f, topic=%s\n",
               verbose ? "true" : "false", (long long)rate_hz, scale, topic);

        // Demonstrate parameter modification
        nano_ros_param_set_bool(&params, "verbose", true);
        nano_ros_param_get_bool(&params, "verbose", &verbose);
        printf("After set: verbose=%s\n", verbose ? "true" : "false");

        // Clean up (parameters are local demo only)
        (void)nano_ros_param_server_fini(&params);
    }

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
    ret = nano_ros_node_init(&node, &support, "c_talker", "/");
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Node created: %s\n", nano_ros_node_get_name(&node));

    // Create publisher
    nano_ros_publisher_t publisher = nano_ros_publisher_get_zero_initialized();
    ret = nano_ros_publisher_init(&publisher, &node, &std_msgs_Int32_type, "/chatter");
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize publisher: %d\n", ret);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Publisher created for topic: %s\n", nano_ros_publisher_get_topic_name(&publisher));

    // Create application context
    talker_context_t talker_ctx = {
        .publisher = &publisher,
        .message = {0},
        .count = 0,
    };
    std_msgs_Int32_init(&talker_ctx.message);

    // Create timer (1 second period = 1,000,000,000 ns)
    nano_ros_timer_t timer = nano_ros_timer_get_zero_initialized();
    ret = nano_ros_timer_init(&timer, &support, 1000000000ULL, timer_callback, &talker_ctx);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize timer: %d\n", ret);
        nano_ros_publisher_fini(&publisher);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Timer created (1 second period)\n");

    // Create executor
    nano_ros_executor_t executor = nano_ros_executor_get_zero_initialized();
    ret = nano_ros_executor_init(&executor, &support, 4);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nano_ros_timer_fini(&timer);
        nano_ros_publisher_fini(&publisher);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    g_executor = &executor;

    // Add timer to executor
    ret = nano_ros_executor_add_timer(&executor, &timer);
    if (ret != NANO_ROS_RET_OK) {
        fprintf(stderr, "Failed to add timer to executor: %d\n", ret);
        nano_ros_executor_fini(&executor);
        nano_ros_timer_fini(&timer);
        nano_ros_publisher_fini(&publisher);
        nano_ros_node_fini(&node);
        nano_ros_support_fini(&support);
        return 1;
    }
    printf("Executor created with %d handle(s)\n", nano_ros_executor_get_handle_count(&executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nPublishing messages (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    ret = nano_ros_executor_spin_period(&executor, 100000000ULL);
    if (ret != NANO_ROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    nano_ros_executor_fini(&executor);
    nano_ros_timer_fini(&timer);
    nano_ros_publisher_fini(&publisher);
    nano_ros_node_fini(&node);
    nano_ros_support_fini(&support);

    printf("Goodbye!\n");
    return 0;
}
