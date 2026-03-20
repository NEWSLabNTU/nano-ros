/// @file main.c
/// @brief ThreadX Linux C action server — Fibonacci on /fibonacci

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/action.h>
#include <nros/executor.h>

#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int goal_count;
} server_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_server_t action_server;
    nros_executor_t executor;
    server_context_t ctx;
} app;

// ----------------------------------------------------------------------------
// Action callbacks
// ----------------------------------------------------------------------------

static nros_goal_response_t goal_callback(const nros_goal_uuid_t *goal_uuid,
                                          const uint8_t *goal_request, size_t goal_len,
                                          void *context) {
    (void)context;

    example_interfaces_action_fibonacci_goal goal;
    if (example_interfaces_action_fibonacci_goal_deserialize(
            &goal, goal_request, goal_len) != 0) {
        printf("Failed to deserialize goal\n");
        return NROS_GOAL_REJECT;
    }

    printf("Goal request: order=%d (uuid=%02x%02x...)\n",
           goal.order, goal_uuid->uuid[0], goal_uuid->uuid[1]);

    if (goal.order < 0 || goal.order >= 64) {
        printf("  -> REJECTED (order out of range)\n");
        return NROS_GOAL_REJECT;
    }

    printf("  -> ACCEPTED\n");
    return NROS_GOAL_ACCEPT_AND_EXECUTE;
}

static nros_cancel_response_t cancel_callback(nros_goal_handle_t *goal, void *context) {
    (void)context;
    (void)goal;
    return NROS_CANCEL_ACCEPT;
}

static void accepted_callback(nros_goal_handle_t *goal, void *context) {
    server_context_t *ctx = (server_context_t *)context;
    ctx->goal_count++;

    printf("Executing goal [%d]\n", ctx->goal_count);

    int32_t order = 5;

    nros_ret_t ret = nros_action_execute(goal);
    if (ret != NROS_RET_OK) {
        printf("Failed to set executing state: %d\n", ret);
        return;
    }

    example_interfaces_action_fibonacci_feedback fb;
    example_interfaces_action_fibonacci_feedback_init(&fb);

    for (int32_t i = 0; i <= order; i++) {
        int32_t val;
        if (i == 0) {
            val = 0;
        } else if (i == 1) {
            val = 1;
        } else {
            val = fb.sequence.data[i - 1] + fb.sequence.data[i - 2];
        }
        fb.sequence.data[i] = val;
        fb.sequence.size = (uint32_t)(i + 1);

        uint8_t fb_buf[512];
        int32_t fb_len = example_interfaces_action_fibonacci_feedback_serialize(
                &fb, fb_buf, sizeof(fb_buf));
        if (fb_len > 0) {
            ret = nros_action_publish_feedback(goal, fb_buf, (size_t)fb_len);
            if (ret == NROS_RET_OK) {
                printf("  Feedback: [");
                for (uint32_t j = 0; j < fb.sequence.size; j++) {
                    if (j > 0) printf(", ");
                    printf("%d", fb.sequence.data[j]);
                }
                printf("]\n");
            }
        }
    }

    example_interfaces_action_fibonacci_result result;
    example_interfaces_action_fibonacci_result_init(&result);
    result.sequence.size = fb.sequence.size;
    memcpy(result.sequence.data, fb.sequence.data,
           fb.sequence.size * sizeof(int32_t));

    uint8_t result_buf[512];
    int32_t result_len = example_interfaces_action_fibonacci_result_serialize(
            &result, result_buf, sizeof(result_buf));
    if (result_len > 0) {
        ret = nros_action_succeed(goal, result_buf, (size_t)result_len);
        if (ret == NROS_RET_OK) {
            printf("  Goal SUCCEEDED\n");
        }
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

void app_main(void) {
    printf("nros C Action Server (ThreadX Linux)\n");

    memset(&app, 0, sizeof(app));

    nros_action_type_t fibonacci_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_ret_t ret = nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize support: %d\n", ret);
        return;
    }
    printf("Support initialized\n");

    ret = nros_node_init(&app.node, &app.support, "c_action_server", "/");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_action_server_init(&app.action_server, &app.node, "/fibonacci",
                                  &fibonacci_type, goal_callback, cancel_callback,
                                  accepted_callback, &app.ctx);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize action server: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_executor_init(&app.executor, &app.support, 8);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize executor: %d\n", ret);
        nros_action_server_fini(&app.action_server);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_executor_add_action_server(&app.executor, &app.action_server);
    if (ret != NROS_RET_OK) {
        printf("Failed to add action server to executor: %d\n", ret);
        nros_executor_fini(&app.executor);
        nros_action_server_fini(&app.action_server);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    printf("Action server ready on /fibonacci\n");
    printf("Waiting for goals...\n");

    for (int i = 0; i < 50000; i++) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
    }

    printf("Server shutting down.\n");

    nros_executor_fini(&app.executor);
    nros_action_server_fini(&app.action_server);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}
