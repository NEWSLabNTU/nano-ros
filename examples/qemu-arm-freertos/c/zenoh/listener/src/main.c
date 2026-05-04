/// @file main.c
/// @brief FreeRTOS C listener — subscribes to std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>

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
        printf("Received: %d\n", msg.data);
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

void app_main(void) {
    printf("nros C Listener (FreeRTOS)\n");

    memset(&app, 0, sizeof(app));

    NROS_CHECK(nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID));
    NROS_CHECK(nros_node_init(&app.node, &app.support, "c_listener", "/"));
    NROS_CHECK(nros_subscription_init(&app.subscription, &app.node,
                                      std_msgs_msg_int32_get_type_support(),
                                      "/chatter", subscription_callback, &app.ctx));
    NROS_CHECK(nros_executor_init(&app.executor, &app.support, 4));
    NROS_CHECK(nros_executor_add_subscription(&app.executor, &app.subscription,
                                              NROS_EXECUTOR_ON_NEW_DATA));

    printf("\nWaiting for messages...\n\n");

    for (;;) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
    }
}
