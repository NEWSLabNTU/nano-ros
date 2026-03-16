/// @file main.c
/// @brief NuttX C action client example - sends Fibonacci goal, gets result

#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/action.h>

#include "example_interfaces.h"

// NuttX embedded config — matches board crate defaults (client = 192.0.3.11)
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_client_t action_client;
} app;

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros NuttX C Action Client (Fibonacci)\n");
    printf("Locator: %s\n", APP_ZENOH_LOCATOR);

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
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }

    ret = nros_node_init(&app.node, &app.support, "nuttx_c_action_client", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }

    ret = nros_action_client_init(
        &app.action_client, &app.node, "/fibonacci", &fibonacci_type);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize action client: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }

    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;

    uint8_t goal_buf[64];
    int32_t goal_len = example_interfaces_action_fibonacci_goal_serialize(
        &goal, goal_buf, sizeof(goal_buf));
    if (goal_len < 0) {
        fprintf(stderr, "Failed to serialize goal\n");
        goto cleanup;
    }

    printf("Sending goal: order=%d\n", goal.order);

    nros_goal_uuid_t goal_uuid;
    ret = nros_action_send_goal(
        &app.action_client, goal_buf, (size_t)goal_len, &goal_uuid);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to send goal: %d\n", ret);
        goto cleanup;
    }

    printf("Goal accepted! (uuid=%02x%02x%02x%02x...)\n",
           goal_uuid.uuid[0], goal_uuid.uuid[1],
           goal_uuid.uuid[2], goal_uuid.uuid[3]);

    printf("Waiting for result...\n\n");

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(
        &app.action_client, &goal_uuid, &final_status,
        result_buf, sizeof(result_buf), &result_len);

    if (ret == NROS_RET_OK) {
        printf("Result (status=%s): ",
               nros_goal_status_to_string(final_status));

        example_interfaces_action_fibonacci_result result;
        if (example_interfaces_action_fibonacci_result_deserialize(
                &result, result_buf, result_len) == 0) {
            printf("[");
            for (uint32_t i = 0; i < result.sequence.size; i++) {
                if (i > 0) printf(", ");
                printf("%d", result.sequence.data[i]);
            }
            printf("]\n");
        } else {
            printf("(deserialize failed)\n");
        }
    } else if (ret == NROS_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for result\n");
    } else {
        fprintf(stderr, "Failed to get result: %d\n", ret);
    }

cleanup:
    nros_action_client_fini(&app.action_client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return (ret == NROS_RET_OK) ? 0 : 1;
}
