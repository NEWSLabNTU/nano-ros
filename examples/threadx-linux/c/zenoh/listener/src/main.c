/// @file main.c
/// @brief ThreadX Linux C listener — subscribes to std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>

#include <nros/app_config.h>
#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int message_count;
} listener_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_subscription_t subscription;
    nros_executor_t executor;
    listener_context_t ctx;
} app;

// ----------------------------------------------------------------------------
// Subscription callback
// ----------------------------------------------------------------------------

static void subscription_callback(const uint8_t *data, size_t len, void *context) {
    listener_context_t *ctx = (listener_context_t *)context;

    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    if (std_msgs_msg_int32_deserialize(&msg, data, len) == 0) {
        ctx->message_count++;
        printf("Received [%d]: %d\n", ctx->message_count, msg.data);
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C Listener (ThreadX Linux)\n");

    memset(&app, 0, sizeof(app));

    NROS_CHECK_RET(nros_support_init(&app.support, NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_listener", "/"), 1);
    NROS_CHECK_RET(nros_subscription_init(&app.subscription, &app.node,
                                      std_msgs_msg_int32_get_type_support(),
                                      "/chatter", subscription_callback, &app.ctx), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_subscription(&app.executor, &app.subscription,
                                              NROS_EXECUTOR_ON_NEW_DATA), 1);

    printf("\nWaiting for messages...\n\n");

    for (int i = 0; i < 100000; i++) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
        if (app.ctx.message_count >= 10) {
            break;
        }
    }

    printf("Received %d messages total.\n", app.ctx.message_count);

    nros_executor_fini(&app.executor);
    nros_subscription_fini(&app.subscription);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}

NROS_APP_MAIN_REGISTER_VOID()
