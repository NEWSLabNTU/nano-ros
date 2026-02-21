/// @file main.c
/// @brief C talker example - publishes std_msgs/Int32 messages using timer

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>
#include <nros/executor.h>
#include <nros/clock.h>
#include <nros/parameter.h>

// Generated message bindings
#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    nros_publisher_t* publisher;
    std_msgs_msg_int32 message;
    int count;
} talker_context_t;

// Static allocation — all nros structs live in .bss, not on the stack
static struct {
    nros_clock_t clock;
    nros_parameter_t param_storage[8];
    nros_param_server_t params;
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t publisher;
    talker_context_t talker_ctx;
    nros_timer_t timer;
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
// Timer callback - publish a message
// ----------------------------------------------------------------------------

static void timer_callback(struct nros_timer_t* timer, void* context) {
    (void)timer;
    talker_context_t* ctx = (talker_context_t*)context;

    ctx->count++;
    ctx->message.data = ctx->count;

    uint8_t buffer[64];
    size_t serialized_size = 0;
    int32_t ret = std_msgs_msg_int32_serialize(&ctx->message, buffer, sizeof(buffer), &serialized_size);

    if (ret == 0 && serialized_size > 0) {
        nros_ret_t pub_ret = nros_publish_raw(ctx->publisher, buffer, serialized_size);
        if (pub_ret == NROS_RET_OK) {
            printf("Published: %d\n", ctx->message.data);
        } else {
            fprintf(stderr, "Publish failed: %d\n", pub_ret);
        }
    } else {
        fprintf(stderr, "Serialize failed: %d\n", ret);
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C Talker\n");
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

    // Zero-initialize all static state (avoids return-by-value temporaries on stack)
    memset(&app, 0, sizeof(app));

    // Demo: Initialize and use clock API
    nros_ret_t clock_ret = nros_clock_init(&app.clock, NROS_CLOCK_SYSTEM_TIME);
    if (clock_ret == NROS_RET_OK) {
        nros_time_t now;
        if (nros_clock_get_now(&app.clock, &now) == NROS_RET_OK) {
            printf("System time: %d.%09u sec\n", now.sec, now.nanosec);
        }
        (void)nros_clock_fini(&app.clock);
    }

    // Demo: Initialize and use parameter server
    if (nros_param_server_init(&app.params, app.param_storage, 8) == NROS_RET_OK) {
        // Declare parameters with default values
        nros_param_declare_bool(&app.params, "verbose", false);
        nros_param_declare_integer(&app.params, "publish_rate_hz", 1);
        nros_param_declare_double(&app.params, "scale_factor", 1.0);
        nros_param_declare_string(&app.params, "topic_name", "/chatter");

        // Read back and display parameter values
        bool verbose = false;
        int64_t rate_hz = 0;
        double scale = 0.0;
        char topic[64] = {0};

        nros_param_get_bool(&app.params, "verbose", &verbose);
        nros_param_get_integer(&app.params, "publish_rate_hz", &rate_hz);
        nros_param_get_double(&app.params, "scale_factor", &scale);
        nros_param_get_string(&app.params, "topic_name", topic, sizeof(topic));

        printf("Parameters: verbose=%s, rate=%lld Hz, scale=%.2f, topic=%s\n",
               verbose ? "true" : "false", (long long)rate_hz, scale, topic);

        // Demonstrate parameter modification
        nros_param_set_bool(&app.params, "verbose", true);
        nros_param_get_bool(&app.params, "verbose", &verbose);
        printf("After set: verbose=%s\n", verbose ? "true" : "false");

        // Clean up (parameters are local demo only)
        (void)nros_param_server_fini(&app.params);
    }

    // Initialize support context
    nros_ret_t ret = nros_support_init(&app.support, locator, domain_id);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }
    printf("Support initialized\n");

    // Create node
    ret = nros_node_init(&app.node, &app.support, "c_talker", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    // Create publisher using generated type support
    ret = nros_publisher_init(&app.publisher, &app.node,
        std_msgs_msg_int32_get_type_support(), "/chatter");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize publisher: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Publisher created for topic: %s\n", nros_publisher_get_topic_name(&app.publisher));

    // Create application context
    app.talker_ctx = (talker_context_t){
        .publisher = &app.publisher,
        .message = {0},
        .count = 0,
    };
    std_msgs_msg_int32_init(&app.talker_ctx.message);

    // Create timer (1 second period = 1,000,000,000 ns)
    ret = nros_timer_init(&app.timer, &app.support, 1000000000ULL, timer_callback, &app.talker_ctx);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize timer: %d\n", ret);
        nros_publisher_fini(&app.publisher);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Timer created (1 second period)\n");

    // Create executor
    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nros_timer_fini(&app.timer);
        nros_publisher_fini(&app.publisher);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }
    g_executor = &app.executor;

    // Add timer to executor
    ret = nros_executor_add_timer(&app.executor, &app.timer);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to add timer to executor: %d\n", ret);
        nros_executor_fini(&app.executor);
        nros_timer_fini(&app.timer);
        nros_publisher_fini(&app.publisher);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Executor created with %d handle(s)\n", nros_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nPublishing messages (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    ret = nros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    nros_executor_fini(&app.executor);
    nros_timer_fini(&app.timer);
    nros_publisher_fini(&app.publisher);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}
