/// @file main.c
/// @brief ThreadX RISC-V QEMU C action client — sends Fibonacci goal to /fibonacci

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/app_main.h>
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

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C Action Client (ThreadX RISC-V QEMU)\n");

    memset(&app, 0, sizeof(app));

    nros_action_type_t fibonacci_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    NROS_CHECK_RET(nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_action_client", "/"), 1);
    NROS_CHECK_RET(nros_action_client_init(&app.action_client, &app.node, "/fibonacci",
                                       &fibonacci_type), 1);
    NROS_SOFTCHECK(nros_action_client_set_feedback_callback(&app.action_client,
                                                            feedback_callback, NULL));
    NROS_SOFTCHECK(nros_action_client_set_result_callback(&app.action_client,
                                                          result_callback, NULL));
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_action_client(&app.executor, &app.action_client), 1);
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

    nros_goal_uuid_t goal_uuid;
    // Retry send_goal — Zenoh discovery may need time to find the server
    for (int attempt = 0; attempt < 5; attempt++) {
        ret = nros_action_send_goal(&app.action_client, &app.executor, goal_buf,
                                    (size_t)goal_len, &goal_uuid);
        if (ret == NROS_RET_OK) {
            break;
        }
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

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(&app.action_client, &app.executor, &goal_uuid,
                                 &final_status, result_buf, sizeof(result_buf),
                                 &result_len);

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

NROS_APP_MAIN_REGISTER_VOID()
