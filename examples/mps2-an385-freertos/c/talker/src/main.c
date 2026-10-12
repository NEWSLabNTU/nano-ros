/// @file main.c
/// @brief C talker example - publishes std_msgs/String "Hello World: N" using a timer

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// rclc include paths; nros/ holds the nano-ros-only entry and check helpers
#include <nros/app_main.h>
#include <nros/check.h>
#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/publisher.h>
#include <rclc/timer.h>

// Generated message bindings
#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    rcl_publisher_t* publisher;
    std_msgs__msg__String message;
    // The bytes behind `message.data` (phase-492 W7): the caller owns them,
    // wired by `std_msgs__msg__String__init_in` below. Nothing allocates.
    std_msgs__msg__String__Storage message_storage;
    int count;
} talker_context_t;

// Static allocation — all nros structs live in .bss, not on the stack
static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_publisher_t publisher;
    talker_context_t talker_ctx;
    rcl_timer_t timer;
    rclc_executor_t executor;
} app;

static volatile sig_atomic_t g_running = 1;
static rclc_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        rclc_executor_cancel(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Timer callback - publish a message
// ----------------------------------------------------------------------------

static void timer_callback(struct rcl_timer_t* timer, void* context) {
    (void)timer;
    talker_context_t* ctx = (talker_context_t*)context;

    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker (demo_nodes_cpp `talker.cpp`).
    ctx->count++;
    snprintf(ctx->message.data.data, ctx->message.data.capacity, "Hello World: %d", ctx->count);
    ctx->message.data.size = strlen(ctx->message.data.data);
    RCLC_SOFTCHECK(rcl_publish(ctx->publisher, &ctx->message, NULL));
    printf("Publishing: '%s'\n", ctx->message.data.data);
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    // Get configuration from environment
    const char* locator = getenv("NROS_LOCATOR");
    if (!locator) {
        locator = NROS_ENTRY_LOCATOR;
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = (uint8_t)NROS_ENTRY_DOMAIN_ID;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    // Zero-initialize all static state (avoids return-by-value temporaries on stack)
    memset(&app, 0, sizeof(app));

    // Initialize support context
    RCLC_CHECK_RET(rclc_support_init_with_locator(&app.support, locator, domain_id), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "talker", "/", &app.support), 1);

    RCLC_CHECK_RET(rclc_publisher_init_default(&app.publisher, &app.node,
                                               ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String),
                                               "/chatter"),
                   1);

    // Create application context
    app.talker_ctx = (talker_context_t){
        .publisher = &app.publisher,
        .message = {0},
        .count = 0,
    };
    std_msgs__msg__String__init_in(&app.talker_ctx.message, &app.talker_ctx.message_storage);

    RCLC_CHECK_RET(
        rclc_timer_init_with_context(&app.timer, 1000000000ULL, timer_callback, &app.talker_ctx),
        1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    g_executor = &app.executor;
    RCLC_CHECK_RET(rclc_executor_add_timer(&app.executor, &app.timer), 1);

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    // Spin with 100ms period
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    rclc_executor_fini(&app.executor);
    rcl_timer_fini(&app.timer);
    rcl_publisher_fini(&app.publisher, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    return 0;
}

NROS_APP_MAIN_REGISTER()
