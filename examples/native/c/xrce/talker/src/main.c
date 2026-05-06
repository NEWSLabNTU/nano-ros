/// @file main.c
/// @brief C XRCE-DDS talker example - publishes std_msgs/Int32 messages using timer
///
/// Build nros-c with XRCE features:
///   cargo build -p nros-c --release --features "rmw-xrce,xrce-udp,platform-posix,ros-humble"
///
/// Run with MicroXRCEAgent:
///   MicroXRCEAgent udp4 -p 2019
///   ./c_xrce_talker

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
#include <nros/publisher.h>
#include <nros/timer.h>

// ----------------------------------------------------------------------------
// std_msgs/Int32 message support (manual definition for this example)
// In a full setup, this would be auto-generated
// ----------------------------------------------------------------------------

typedef struct std_msgs_Int32 {
    int32_t data;
} std_msgs_Int32;

// Serialize to CDR format
// CDR format for Int32: 4-byte header (0x00, 0x01, 0x00, 0x00) + 4-byte int32
static int32_t std_msgs_Int32_serialize(const std_msgs_Int32* msg, uint8_t* buffer,
                                        size_t buffer_size) {
    if (buffer_size < 8) {
        return -1;
    }
    // CDR header (big-endian format flag = 0x00, little-endian = 0x01)
    buffer[0] = 0x00; // CDR encapsulation
    buffer[1] = 0x01; // Little-endian
    buffer[2] = 0x00; // Reserved
    buffer[3] = 0x00; // Reserved
    // Little-endian int32
    buffer[4] = (uint8_t)(msg->data & 0xFF);
    buffer[5] = (uint8_t)((msg->data >> 8) & 0xFF);
    buffer[6] = (uint8_t)((msg->data >> 16) & 0xFF);
    buffer[7] = (uint8_t)((msg->data >> 24) & 0xFF);
    return 8;
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
    nros_publisher_t* publisher;
    std_msgs_Int32 message;
    int count;
} talker_context_t;

// Static allocation — all nros structs live in .bss, not on the stack
static struct {
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
    int32_t len = std_msgs_Int32_serialize(&ctx->message, buffer, sizeof(buffer));

    if (len > 0) {
        nros_ret_t ret = nros_publish_raw(ctx->publisher, buffer, (size_t)len);
        if (ret == NROS_RET_OK) {
            printf("Published: %d\n", ctx->message.data);
        } else {
            fprintf(stderr, "Publish failed: %d\n", ret);
        }
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C XRCE-DDS Talker\n");
    printf("======================\n");

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
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_xrce_talker", "/"), 1);
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    NROS_CHECK_RET(nros_publisher_init(&app.publisher, &app.node, &std_msgs_Int32_type, "/chatter"),
                   1);
    printf("Publisher created for topic: %s\n", nros_publisher_get_topic_name(&app.publisher));

    app.talker_ctx = (talker_context_t){
        .publisher = &app.publisher,
        .message = {0},
        .count = 0,
    };

    NROS_CHECK_RET(
        nros_timer_init(&app.timer, &app.support, 1000000000ULL, timer_callback, &app.talker_ctx),
        1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    NROS_CHECK_RET(nros_executor_add_timer(&app.executor, &app.timer), 1);
    printf("Executor created with %d handle(s)\n", nros_executor_get_handle_count(&app.executor));

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nPublishing messages (Ctrl+C to exit)...\n\n");

    nros_ret_t ret = nros_executor_spin_period(&app.executor, 100000000ULL);
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

NROS_APP_MAIN_REGISTER_POSIX()
