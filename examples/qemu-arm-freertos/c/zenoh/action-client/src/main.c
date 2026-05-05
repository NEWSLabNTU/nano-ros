/// @file main.c
/// @brief FreeRTOS C action client — sends Fibonacci goal to /fibonacci

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/action.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>

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

    NROS_CHECK(nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID));
    NROS_CHECK(nros_node_init(&app.node, &app.support, "c_action_client", "/"));
    NROS_CHECK(nros_action_client_init(&app.action_client, &app.node, "/fibonacci",
                                       &fibonacci_type));
    NROS_CHECK(nros_executor_init(&app.executor, &app.support, 8));
    // Register action client with executor (creates transport handles in arena)
    NROS_CHECK(nros_executor_add_action_client(&app.executor, &app.action_client));
    nros_ret_t ret = NROS_RET_OK;

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

    // Blocking send_goal — spins the executor internally until
    // the server accepts/rejects, or timeout. No zpico_get condvar.
    nros_goal_uuid_t goal_uuid;
    for (int attempt = 0; attempt < 5; attempt++) {
        ret = nros_action_send_goal(&app.action_client, &app.executor,
                                    goal_buf, (size_t)goal_len, &goal_uuid);
        if (ret == NROS_RET_OK) break;
        printf("Goal attempt %d failed (%d), retrying...\n", attempt + 1, ret);
        for (int j = 0; j < 500; j++) {
            nros_executor_spin_some(&app.executor, 10000000ULL);
        }
    }

    if (ret != NROS_RET_OK) {
        printf("Failed to send goal after retries: %d\n", ret);
        goto cleanup;
    }

    printf("Goal accepted!\n");
    printf("Waiting for result...\n\n");

    // Blocking get_result — spins the executor internally.
    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(&app.action_client, &app.executor,
                                 &goal_uuid, &final_status,
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
    nros_executor_fini(&app.executor);
    nros_action_client_fini(&app.action_client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}
