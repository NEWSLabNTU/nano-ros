/// @file main.c
/// @brief C listener example - subscribes to std_msgs/String messages

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
#include <rclc/subscription.h>

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
    rclc_support_t support;
    rcl_node_t node;
    listener_context_t listener_ctx;
    rcl_subscription_t subscription;
    rclc_executor_t executor;
    // The message the callback is handed. It is OURS — that is the whole
    // mechanism behind typed delivery on a path with no allocator, and it is
    // rclc's: the executor deserializes into this object and calls us, rather
    // than allocating one per sample. It must outlive the subscription, which
    // here is trivially true because both are in this one `.bss` object.
    std_msgs__msg__String msg;
    // The bytes behind `msg.data` (phase-492 W7). The deserializer writes into
    // the `capacity` these give and refuses a longer sample; nothing allocates.
    std_msgs__msg__String__Storage msg_storage;
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
// Subscription callback - process received message
// ----------------------------------------------------------------------------

// phase-417 W5.a — rclc's callback shape: a DESERIALIZED message, not CDR
// bytes. `msgin` is `&app.msg`, written by the generated
// `std_msgs__msg__String__deserialize` immediately before this call, so this body
// casts and reads FIELDS and there is no CDR anywhere in it.
//
// A sample that cannot be decoded into `app.msg` — a string longer than the
// bound `nros-codegen.toml` declares included — does NOT reach here: the
// executor drops it, counts a subscription error and logs it. That is why this
// function has no failure branch rather than a relocated one; calling it after
// a failed decode would hand it the PREVIOUS message dressed as the new one.
static void subscription_callback(const void* msgin, void* context) {
    const std_msgs__msg__String* msg = (const std_msgs__msg__String*)msgin;
    listener_context_t* ctx = (listener_context_t*)context;

    ctx->message_count++;
    printf("I heard: [%s]\n", msg->data.data);
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout so each printf flushes on its newline. When stdout is
    // a pipe (e.g. a test harness capturing output) glibc defaults to 4 KiB
    // block buffering, so "Received: N" lines would sit unflushed for a long
    // time and an observer waiting on them sees nothing. Line buffering makes
    // the output appear live, matching interactive (tty) behaviour.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    printf("nros C Listener\n");
    printf("===================\n");

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

    printf("Locator: %s\n", locator);
    printf("Domain ID: %d\n", domain_id);

    // Zero-initialize all static state (avoids return-by-value temporaries on stack)
    memset(&app, 0, sizeof(app));

    // Initialize support context
    RCLC_CHECK_RET(rclc_support_init_with_locator(&app.support, locator, domain_id), 1);
    printf("Support initialized\n");
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "listener", "/", &app.support), 1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    // Create application context
    app.listener_ctx = (listener_context_t){
        .message_count = 0,
    };

    RCLC_CHECK_RET(rclc_subscription_init_default(
                       &app.subscription, &app.node,
                       ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/chatter"),
                   1);
    printf("Subscriber created for topic: %s\n",
           rcl_subscription_get_topic_name(&app.subscription));

    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    g_executor = &app.executor;
    /* phase-417 W5.a — rclc's registration, argument for argument:
     *
     *   rclc_executor_add_subscription_with_context(&exec, &sub, &msg, &cb, &ctx, ON_NEW_DATA);
     *
     * phase-492 W8: the deserializer and the receive-buffer hint come from the
     * descriptor the subscription was created with
     * (`ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String)`), so this is rclc's
     * own call. `app.msg` is wired to `app.msg_storage` first. The
     * byte-oriented `rclc_executor_add_subscription_raw` is still there for a
     * caller doing its own CDR; this file no longer is one. */
    std_msgs__msg__String__init_in(&app.msg, &app.msg_storage);
    RCLC_CHECK_RET(rclc_executor_add_subscription_with_context(&app.executor, &app.subscription,
                                                               &app.msg, subscription_callback,
                                                               &app.listener_ctx, ON_NEW_DATA),
                   1);
    printf("Executor created with %d handle(s)\n", rclc_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total messages received: %d\n", app.listener_ctx.message_count);
    rclc_executor_fini(&app.executor);
    rcl_subscription_fini(&app.subscription, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER()
