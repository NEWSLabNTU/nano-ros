/// @file main.c
/// @brief NuttX C listener example - subscribes to std_msgs/Int32 on /chatter

#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/subscription.h>
#include <nros/executor.h>

#include "std_msgs.h"

// NuttX embedded config — matches board crate defaults (listener = 192.0.3.11)
#define ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#define DOMAIN_ID 0
#define MAX_MESSAGES 10

typedef struct {
    int message_count;
} listener_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    listener_context_t listener_ctx;
    nros_subscription_t subscription;
    nros_executor_t executor;
} app;

static void subscription_callback(const uint8_t* data, size_t len, void* context) {
    listener_context_t* ctx = (listener_context_t*)context;

    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    if (std_msgs_msg_int32_deserialize(&msg, data, len) == 0) {
        ctx->message_count++;
        printf("Received [%d]: %d\n", ctx->message_count, msg.data);

        if (ctx->message_count >= MAX_MESSAGES) {
            printf("\nReceived %d messages.\n", MAX_MESSAGES);
            nros_executor_stop(&app.executor);
        }
    } else {
        fprintf(stderr, "Failed to deserialize message\n");
    }
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros NuttX C Listener\n");
    printf("Locator: %s\n", ZENOH_LOCATOR);

    memset(&app, 0, sizeof(app));

    nros_ret_t ret = nros_support_init(&app.support, ZENOH_LOCATOR, DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }

    ret = nros_node_init(&app.node, &app.support, "nuttx_c_listener", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }

    app.listener_ctx = (listener_context_t){ .message_count = 0 };

    ret = nros_subscription_init(
        &app.subscription,
        &app.node,
        std_msgs_msg_int32_get_type_support(),
        "/chatter",
        subscription_callback,
        &app.listener_ctx
    );
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize subscription: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nros_subscription_fini(&app.subscription);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    nros_executor_add_subscription(&app.executor, &app.subscription,
        NROS_EXECUTOR_ON_NEW_DATA);

    printf("Waiting for messages...\n\n");
    nros_executor_spin_period(&app.executor, 100000000ULL);

    nros_executor_fini(&app.executor);
    nros_subscription_fini(&app.subscription);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return 0;
}
