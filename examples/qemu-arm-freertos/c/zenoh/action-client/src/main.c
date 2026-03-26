/// @file main.c
/// @brief FreeRTOS C action client — sends Fibonacci goal to /fibonacci (async API)

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

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_client_t action_client;
    nros_executor_t executor;
} app;

static volatile int g_goal_accepted = -1;  // -1=pending, 0=rejected, 1=accepted
static volatile int g_result_received = 0;
static volatile int g_feedback_count = 0;
static nros_goal_uuid_t g_goal_uuid;

// ----------------------------------------------------------------------------
// Async callbacks (invoked during nros_executor_spin_some)
// ----------------------------------------------------------------------------

static void goal_response_callback(const nros_goal_uuid_t *goal_uuid,
                                   bool accepted, void *context) {
    (void)goal_uuid;
    (void)context;
    g_goal_accepted = accepted ? 1 : 0;
    if (accepted) {
        printf("Goal accepted!\n");
        // Automatically request the result
        nros_action_get_result_async(&app.action_client, goal_uuid);
    } else {
        printf("Goal rejected!\n");
    }
}

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

static void result_callback(const nros_goal_uuid_t *goal_uuid,
                            nros_goal_status_t status,
                            const uint8_t *result, size_t result_len,
                            void *context) {
    (void)goal_uuid;
    (void)context;
    (void)status;

    example_interfaces_action_fibonacci_result res;
    if (example_interfaces_action_fibonacci_result_deserialize(
            &res, result, result_len) == 0) {
        printf("Result: [");
        for (uint32_t i = 0; i < res.sequence.size; i++) {
            if (i > 0) printf(", ");
            printf("%d", res.sequence.data[i]);
        }
        printf("]\n");
    }

    printf("\nAction completed successfully.\n");
    g_result_received = 1;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

void app_main(void) {
    printf("nros C Action Client (FreeRTOS) [async]\n");

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

    // Register async callbacks
    nros_action_client_set_goal_response_callback(&app.action_client,
                                                   goal_response_callback, NULL);
    nros_action_client_set_feedback_callback(&app.action_client,
                                              feedback_callback, NULL);
    nros_action_client_set_result_callback(&app.action_client,
                                            result_callback, NULL);

    ret = nros_executor_init(&app.executor, &app.support, 8);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize executor: %d\n", ret);
        nros_action_client_fini(&app.action_client);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    // Register action client with executor for async polling
    ret = nros_executor_add_action_client(&app.executor, &app.action_client);
    if (ret != NROS_RET_OK) {
        printf("Failed to add action client to executor: %d\n", ret);
        goto cleanup;
    }

    printf("Action client ready for /fibonacci\n");

    // Warm-up: spin to allow Zenoh to discover the server's queryables
    for (int i = 0; i < 500; i++) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
    }

    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 5;

    uint8_t goal_buf[64];
    int32_t goal_len = example_interfaces_action_fibonacci_goal_serialize(
            &goal, goal_buf, sizeof(goal_buf));
    if (goal_len < 0) {
        printf("Failed to serialize goal\n");
        goto cleanup;
    }

    printf("Sending goal: order=%d\n", goal.order);

    // Send goal asynchronously — returns immediately.
    // Response arrives via goal_response_callback during spin.
    ret = nros_action_send_goal_async(&app.action_client, goal_buf, (size_t)goal_len,
                                      &g_goal_uuid);
    if (ret != NROS_RET_OK) {
        printf("Failed to send goal: %d\n", ret);
        goto cleanup;
    }

    // Spin until result received or timeout (30s = 3000 × 10ms)
    for (int i = 0; i < 3000 && !g_result_received; i++) {
        nros_executor_spin_some(&app.executor, 10000000ULL);
    }

    if (!g_result_received) {
        printf("Timeout waiting for result\n");
    }

cleanup:
    nros_executor_fini(&app.executor);
    nros_action_client_fini(&app.action_client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}
