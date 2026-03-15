/// @file main.c
/// @brief FreeRTOS C action client — sends Fibonacci goal to /fibonacci

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/action.h>

#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_client_t action_client;
} app;

static int g_feedback_count = 0;

// ----------------------------------------------------------------------------
// Feedback callback
// ----------------------------------------------------------------------------

static void feedback_callback(const nros_goal_uuid_t *goal_uuid,
                              const uint8_t *feedback, size_t feedback_len,
                              void *context) {
    (void)goal_uuid;
    (void)context;

    g_feedback_count++;

    example_interfaces_action_fibonacci_feedback fb;
    if (example_interfaces_action_fibonacci_feedback_deserialize(
            &fb, feedback, feedback_len) == 0) {
        printf("Feedback #%d: [", g_feedback_count);
        for (uint32_t i = 0; i < fb.sequence.size; i++) {
            if (i > 0) printf(", ");
            printf("%d", fb.sequence.data[i]);
        }
        printf("]\n");
    }
}

// ----------------------------------------------------------------------------
// Result callback
// ----------------------------------------------------------------------------

static void result_callback(const nros_goal_uuid_t *goal_uuid,
                            nros_goal_status_t status,
                            const uint8_t *result, size_t result_len,
                            void *context) {
    (void)goal_uuid;
    (void)context;
    (void)status;
    (void)result;
    (void)result_len;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

void app_main(void) {
    printf("nros C Action Client (FreeRTOS)\n");

    memset(&app, 0, sizeof(app));

    nros_action_type_t fibonacci_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_ret_t ret = nros_support_init(&app.support, "tcp/192.0.3.1:7447", 0);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize support: %d\n", ret);
        return;
    }
    printf("Support initialized\n");

    ret = nros_node_init(&app.node, &app.support, "c_action_client", "/");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_action_client_init(&app.action_client, &app.node, "/fibonacci",
                                  &fibonacci_type);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize action client: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_action_client_set_feedback_callback(&app.action_client,
                                                    feedback_callback, NULL);
    if (ret != NROS_RET_OK) {
        printf("Failed to set feedback callback: %d\n", ret);
    }

    ret = nros_action_client_set_result_callback(&app.action_client,
                                                  result_callback, NULL);
    if (ret != NROS_RET_OK) {
        printf("Failed to set result callback: %d\n", ret);
    }

    printf("Action client ready for /fibonacci\n");

    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 5;

    uint8_t goal_buf[64];
    size_t goal_size = 0;
    if (example_interfaces_action_fibonacci_goal_serialize(
            &goal, goal_buf, sizeof(goal_buf), &goal_size) != 0) {
        printf("Failed to serialize goal\n");
        goto cleanup;
    }

    printf("Sending goal: order=%d\n", goal.order);

    nros_goal_uuid_t goal_uuid;
    ret = nros_action_send_goal(&app.action_client, goal_buf, goal_size,
                                &goal_uuid);

    if (ret != NROS_RET_OK) {
        printf("Failed to send goal: %d\n", ret);
        goto cleanup;
    }

    printf("Goal accepted!\n");
    printf("Waiting for result...\n\n");

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(&app.action_client, &goal_uuid, &final_status,
                                 result_buf, sizeof(result_buf), &result_len);

    if (ret == NROS_RET_OK) {
        example_interfaces_action_fibonacci_result result;
        if (example_interfaces_action_fibonacci_result_deserialize(
                &result, result_buf, result_len) == 0) {
            printf("Result: [");
            for (uint32_t i = 0; i < result.sequence.size; i++) {
                if (i > 0) printf(", ");
                printf("%d", result.sequence.data[i]);
            }
            printf("]\n");
        }
        printf("\nAction completed successfully.\n");
    } else if (ret == NROS_RET_TIMEOUT) {
        printf("Timeout waiting for result\n");
    } else {
        printf("Failed to get result: %d\n", ret);
    }

cleanup:
    nros_action_client_fini(&app.action_client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}
