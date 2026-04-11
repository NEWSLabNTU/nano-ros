/// @file main.c
/// @brief ThreadX RISC-V QEMU C listener — subscribes to std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>
#include <nros/executor.h>

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
    printf("nros C Listener (ThreadX RISC-V QEMU)\n");

    memset(&app, 0, sizeof(app));

    nros_ret_t ret = nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize support: %d\n", ret);
        return;
    }
    printf("Support initialized\n");

    ret = nros_node_init(&app.node, &app.support, "c_listener", "/");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_subscription_init(&app.subscription, &app.node,
                                  std_msgs_msg_int32_get_type_support(),
                                  "/chatter", subscription_callback, &app.ctx);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize subscription: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize executor: %d\n", ret);
        nros_subscription_fini(&app.subscription);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_executor_add_subscription(&app.executor, &app.subscription,
                                          NROS_EXECUTOR_ON_NEW_DATA);
    if (ret != NROS_RET_OK) {
        printf("Failed to add subscription to executor: %d\n", ret);
        nros_executor_fini(&app.executor);
        nros_subscription_fini(&app.subscription);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    printf("\nWaiting for messages...\n\n");

    for (;;) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
    }
}
