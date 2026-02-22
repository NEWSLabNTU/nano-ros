/// @file main.c
/// @brief NuttX C talker example - publishes std_msgs/Int32 on /chatter

#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>
#include <nros/executor.h>

#include "std_msgs.h"

// NuttX embedded config — matches board crate defaults (talker = 192.0.3.10)
#define ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#define DOMAIN_ID 0

typedef struct {
    nros_publisher_t* publisher;
    std_msgs_msg_int32 message;
    int count;
    int max_count;
} talker_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t publisher;
    talker_context_t talker_ctx;
    nros_timer_t timer;
    nros_executor_t executor;
} app;

static void timer_callback(struct nros_timer_t* timer, void* context) {
    (void)timer;
    talker_context_t* ctx = (talker_context_t*)context;

    ctx->count++;
    ctx->message.data = ctx->count;

    uint8_t buffer[64];
    size_t serialized_size = 0;
    int32_t ret = std_msgs_msg_int32_serialize(
        &ctx->message, buffer, sizeof(buffer), &serialized_size);

    if (ret == 0 && serialized_size > 0) {
        nros_ret_t pub_ret = nros_publish_raw(ctx->publisher, buffer, serialized_size);
        if (pub_ret == NROS_RET_OK) {
            printf("Published: %d\n", ctx->message.data);
        } else {
            fprintf(stderr, "Publish failed: %d\n", pub_ret);
        }
    }

    if (ctx->count >= ctx->max_count) {
        printf("\nDone publishing %d messages.\n", ctx->max_count);
        nros_executor_stop(&app.executor);
    }
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros NuttX C Talker\n");
    printf("Locator: %s\n", ZENOH_LOCATOR);

    memset(&app, 0, sizeof(app));

    nros_ret_t ret = nros_support_init(&app.support, ZENOH_LOCATOR, DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }

    ret = nros_node_init(&app.node, &app.support, "nuttx_c_talker", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }

    ret = nros_publisher_init(&app.publisher, &app.node,
        std_msgs_msg_int32_get_type_support(), "/chatter");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize publisher: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    app.talker_ctx = (talker_context_t){
        .publisher = &app.publisher,
        .message = {0},
        .count = 0,
        .max_count = 10,
    };
    std_msgs_msg_int32_init(&app.talker_ctx.message);

    ret = nros_timer_init(&app.timer, &app.support, 1000000000ULL,
        timer_callback, &app.talker_ctx);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize timer: %d\n", ret);
        nros_publisher_fini(&app.publisher);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nros_timer_fini(&app.timer);
        nros_publisher_fini(&app.publisher);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    nros_executor_add_timer(&app.executor, &app.timer);

    printf("Publishing messages...\n\n");
    nros_executor_spin_period(&app.executor, 100000000ULL);

    nros_executor_fini(&app.executor);
    nros_timer_fini(&app.timer);
    nros_publisher_fini(&app.publisher);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return 0;
}
