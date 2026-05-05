/// @file main.c
/// @brief C action server example (XRCE-DDS) - Fibonacci action with feedback

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

#include <nros/app_main.h>
#include <nros/action.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>

#include "example_interfaces.h"

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

static volatile sig_atomic_t g_running = 1;
static nros_executor_t* g_executor = NULL;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nros_executor_stop(g_executor);
    }
}

static nros_goal_response_t goal_callback(nros_action_server_t* server,
                                          const nros_goal_handle_t* goal,
                                          const uint8_t* goal_request, size_t goal_len,
                                          void* context) {
    (void)server;
    (void)context;

    example_interfaces_action_fibonacci_goal goal_msg;
    if (example_interfaces_action_fibonacci_goal_deserialize(&goal_msg, goal_request, goal_len) !=
        0) {
        fprintf(stderr, "Failed to deserialize goal\n");
        return NROS_GOAL_REJECT;
    }

    printf("Goal request: order=%d (uuid=%02x%02x...)\n", goal_msg.order, goal->uuid.uuid[0],
           goal->uuid.uuid[1]);

    if (goal_msg.order < 0 || goal_msg.order >= 64) {
        printf("  -> REJECTED (order out of range)\n");
        return NROS_GOAL_REJECT;
    }

    printf("  -> ACCEPTED\n");
    return NROS_GOAL_ACCEPT_AND_EXECUTE;
}

static nros_cancel_response_t cancel_callback(nros_action_server_t* server,
                                              const nros_goal_handle_t* goal, void* context) {
    (void)server;
    (void)context;
    printf("Cancel request for goal (uuid=%02x%02x...)\n", goal->uuid.uuid[0], goal->uuid.uuid[1]);
    return NROS_CANCEL_ACCEPT;
}

static void accepted_callback(nros_action_server_t* server, const nros_goal_handle_t* goal,
                              void* context) {
    (void)server;
    server_context_t* ctx = (server_context_t*)context;
    ctx->goal_count++;

    printf("Executing goal [%d] (uuid=%02x%02x...)\n", ctx->goal_count, goal->uuid.uuid[0],
           goal->uuid.uuid[1]);

    int32_t order = 10;

    nros_ret_t ret = nros_action_execute(server, goal);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to set executing state: %d\n", ret);
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
        int32_t fb_len =
            example_interfaces_action_fibonacci_feedback_serialize(&fb, fb_buf, sizeof(fb_buf));
        if (fb_len > 0) {
            ret = nros_action_publish_feedback(server, goal, fb_buf, (size_t)fb_len);
            if (ret != NROS_RET_OK) {
                fprintf(stderr, "Failed to publish feedback: %d\n", ret);
            }
        }
    }

    example_interfaces_action_fibonacci_result result;
    example_interfaces_action_fibonacci_result_init(&result);
    result.sequence.size = fb.sequence.size;
    memcpy(result.sequence.data, fb.sequence.data, fb.sequence.size * sizeof(int32_t));

    uint8_t result_buf[512];
    int32_t result_len = example_interfaces_action_fibonacci_result_serialize(&result, result_buf,
                                                                              sizeof(result_buf));
    if (result_len > 0) {
        ret = nros_action_succeed(server, goal, result_buf, (size_t)result_len);
        if (ret != NROS_RET_OK) {
            fprintf(stderr, "Failed to send result: %d\n", ret);
        } else {
            printf("  Goal SUCCEEDED\n");
        }
    }
}

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C Action Server - XRCE (Fibonacci)\n");
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

    NROS_CHECK_RET(nros_support_init(&app.support, agent, domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_xrce_action_server", "/"), 1);
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    NROS_CHECK_RET(nros_action_server_init(&app.action_server, &app.node, "/fibonacci",
                                           &fibonacci_type, goal_callback, cancel_callback,
                                           accepted_callback, &app.ctx), 1);
    printf("Action server created: /fibonacci\n");

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 8), 1);
    g_executor = &app.executor;
    NROS_CHECK_RET(nros_executor_add_action_server(&app.executor, &app.action_server), 1);

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for action goals (Ctrl+C to exit)...\n\n");

    nros_ret_t ret = nros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    printf("\nShutting down...\n");
    printf("Total goals handled: %d\n", app.ctx.goal_count);
    nros_executor_fini(&app.executor);
    nros_action_server_fini(&app.action_server);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
