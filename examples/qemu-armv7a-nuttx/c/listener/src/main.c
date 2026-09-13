/// @file main.c
/// @brief C listener example - subscribes to std_msgs/String messages

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
    nros_support_t support;
    nros_node_t node;
    listener_context_t listener_ctx;
    nros_subscription_t subscription;
    nros_executor_t executor;
    // The message the callback is handed. It is OURS — that is the whole
    // mechanism behind typed delivery on a path with no allocator, and it is
    // rclc's: the executor deserializes into this object and calls us, rather
    // than allocating one per sample. It must outlive the subscription, which
    // here is trivially true because both are in this one `.bss` object.
    std_msgs_msg_string msg;
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
        nros_executor_cancel(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Subscription callback - process received message
// ----------------------------------------------------------------------------

// phase-417 W5.a — rclc's callback shape: a DESERIALIZED message, not CDR
// bytes. `msgin` is `&app.msg`, written by the generated
// `std_msgs_msg_string_deserialize` immediately before this call, so this body
// casts and reads FIELDS and there is no CDR anywhere in it.
//
// A sample that cannot be decoded into `app.msg` — a string longer than the
// bound `nros-codegen.toml` declares included — does NOT reach here: the
// executor drops it, counts a subscription error and logs it. That is why this
// function has no failure branch rather than a relocated one; calling it after
// a failed decode would hand it the PREVIOUS message dressed as the new one.
static void subscription_callback(const void* msgin, void* context) {
    const std_msgs_msg_string* msg = (const std_msgs_msg_string*)msgin;
    listener_context_t* ctx = (listener_context_t*)context;

    ctx->message_count++;
    printf("I heard: [%s]\n", msg->data);
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
    NROS_CHECK_RET(nros_support_init(&app.support, locator, domain_id), 1);
    printf("Support initialized\n");
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "listener", "/", &app.support), 1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    // Create application context
    app.listener_ctx = (listener_context_t){
        .message_count = 0,
    };

    NROS_CHECK_RET(rclc_subscription_init_default(&app.subscription, &app.node,
                                                  std_msgs_msg_string_get_type_support(),
                                                  "/chatter"),
                   1);
    printf("Subscriber created for topic: %s\n",
           rcl_subscription_get_topic_name(&app.subscription));

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    /* phase-417 W5.a — rclc's registration, argument for argument:
     *
     *   rclc_executor_add_subscription_with_context(&exec, &sub, &msg, &cb, &ctx, ON_NEW_DATA);
     *
     * The deserializer and the receive-buffer hint are derived from the type
     * token in the macro's own name, so they cannot disagree with each other or
     * with `app.msg` — which the macro also checks is a `std_msgs_msg_string*`
     * rather than trusting the FFI's `void*`. The byte-oriented
     * `nros_executor_add_subscription_raw` is still there for a caller doing
     * its own CDR; this file no longer is one. */
    NROS_CHECK_RET(std_msgs_msg_string_executor_add_subscription(
                       &app.executor, &app.subscription, &app.msg, subscription_callback,
                       &app.listener_ctx, NROS_EXECUTOR_ON_NEW_DATA),
                   1);
    printf("Executor created with %d handle(s)\n", nros_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    nros_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total messages received: %d\n", app.listener_ctx.message_count);
    rclc_executor_fini(&app.executor);
    nros_subscription_fini(&app.subscription);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER()
