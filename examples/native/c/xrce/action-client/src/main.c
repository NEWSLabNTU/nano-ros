/// @file main.c
/// @brief C action client example (XRCE-DDS) - sends Fibonacci goal, receives feedback

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/action.h>

#include "example_interfaces.h"

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_client_t action_client;
} app;

static volatile sig_atomic_t g_running = 1;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

static void print_sequence(const example_interfaces_action_fibonacci_feedback* fb) {
    printf("[");
    for (uint32_t i = 0; i < fb->sequence.size; i++) {
        if (i > 0) printf(", ");
        printf("%d", fb->sequence.data[i]);
    }
    printf("]");
}

static int g_feedback_count = 0;

static void feedback_callback(const nros_goal_uuid_t* goal_uuid, const uint8_t* feedback,
                              size_t feedback_len, void* context) {
    (void)goal_uuid;
    (void)context;

    g_feedback_count++;

    example_interfaces_action_fibonacci_feedback fb;
    if (example_interfaces_action_fibonacci_feedback_deserialize(&fb, feedback, feedback_len) ==
        0) {
        printf("Feedback #%d: ", g_feedback_count);
        print_sequence(&fb);
        printf("\n");
    } else {
        fprintf(stderr, "Feedback #%d: failed to deserialize\n", g_feedback_count);
    }
}

static int g_result_received = 0;

static void result_callback(const nros_goal_uuid_t* goal_uuid, nros_goal_status_t status,
                            const uint8_t* result, size_t result_len, void* context) {
    (void)goal_uuid;
    (void)context;

    g_result_received = 1;

    printf("Result (status=%s): ", nros_goal_status_to_string(status));

    if (result && result_len > 0) {
        example_interfaces_action_fibonacci_feedback seq;
        if (example_interfaces_action_fibonacci_feedback_deserialize(&seq, result, result_len) ==
            0) {
            print_sequence(&seq);
            printf("\n");
        } else {
            printf("(deserialize failed)\n");
        }
    } else {
        printf("(no result data)\n");
    }
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C Action Client - XRCE (Fibonacci)\n");
    printf("==========================================\n");

    const char* agent = getenv("XRCE_AGENT_ADDR");
    if (!agent) {
        agent = "127.0.0.1:2019";
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Agent: %s\n", agent);
    printf("Domain ID: %d\n", domain_id);

    memset(&app, 0, sizeof(app));

    nros_action_type_t fibonacci_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_ret_t ret = nros_support_init(&app.support, agent, domain_id);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }
    printf("Support initialized\n");

    ret = nros_node_init(&app.node, &app.support, "c_xrce_action_client", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    ret = nros_action_client_init(&app.action_client, &app.node, "/fibonacci", &fibonacci_type);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize action client: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Action client created: /fibonacci\n");

    nros_action_client_set_feedback_callback(&app.action_client, feedback_callback, NULL);
    nros_action_client_set_result_callback(&app.action_client, result_callback, NULL);

    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;

    uint8_t goal_buf[64];
    int32_t goal_len =
        example_interfaces_action_fibonacci_goal_serialize(&goal, goal_buf, sizeof(goal_buf));
    if (goal_len < 0) {
        fprintf(stderr, "Failed to serialize goal\n");
        goto cleanup;
    }

    printf("\nSending goal: order=%d\n", goal.order);

    nros_goal_uuid_t goal_uuid;
    ret = nros_action_send_goal(&app.action_client, goal_buf, (size_t)goal_len, &goal_uuid);

    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to send goal: %d\n", ret);
        goto cleanup;
    }

    printf("Goal sent (uuid=%02x%02x%02x%02x...)\n", goal_uuid.uuid[0], goal_uuid.uuid[1],
           goal_uuid.uuid[2], goal_uuid.uuid[3]);

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for result...\n\n");

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(&app.action_client, &goal_uuid, &final_status, result_buf,
                                 sizeof(result_buf), &result_len);

    if (ret == NROS_RET_OK) {
        printf("Final result (status=%s): ", nros_goal_status_to_string(final_status));

        example_interfaces_action_fibonacci_result result;
        if (example_interfaces_action_fibonacci_result_deserialize(&result, result_buf,
                                                                   result_len) == 0) {
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
    printf("\nShutting down...\n");
    nros_action_client_fini(&app.action_client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return (ret == NROS_RET_OK) ? 0 : 1;
}
