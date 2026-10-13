/// @file main.c
/// @brief C action server example - Fibonacci action with feedback

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// The C API at rcl/rclc's include paths
#include <rclc/action_server.h>
#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>

// Generated C bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int goal_count;
    // issue 0453 — the order most recently ACCEPTED, so `accepted_callback` can
    // compute the sequence the client actually asked for. The goal handle
    // surfaces the uuid and status but not the request payload, which is why
    // this used to hard-code 10: it had nothing else to go on. Mirrors the Rust
    // server's `State` (issue 0450). One slot is enough for the demo (a single
    // goal at a time); a server handling concurrent goals would key this by
    // goal uuid.
    int32_t accepted_order;
} server_context_t;

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_action_server_t action_server;
    rclc_executor_t executor;
    server_context_t ctx;
} app;

static volatile sig_atomic_t g_running = 1;
static rclc_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        rclc_executor_cancel(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Action callbacks
// ----------------------------------------------------------------------------

static rclc_goal_response_t goal_callback(rcl_action_server_t* server,
                                          const rclc_goal_handle_t* goal,
                                          const uint8_t* goal_request, size_t goal_len,
                                          void* context) {
    (void)server;
    (void)goal;

    // Deserialize goal using generated function
    example_interfaces__action__Fibonacci_Goal goal_msg;
    if (example_interfaces__action__Fibonacci_Goal__deserialize(&goal_msg, goal_request,
                                                                goal_len) != 0) {
        fprintf(stderr, "Failed to deserialize goal\n");
        return RCLC_GOAL_REJECT;
    }

    printf("Received goal request with order %d\n", goal_msg.order);

    // Reject negative orders or orders too large
    if (goal_msg.order < 0 || goal_msg.order >= 64) {
        printf("Goal rejected: order out of range\n");
        return RCLC_GOAL_REJECT;
    }

    // issue 0453 — remember what was requested. Reading the order for the
    // range check and then ignoring it when computing the result is what made
    // this server's output independent of its input, so no test could tell a
    // delivered goal from a dropped one.
    server_context_t* ctx = (server_context_t*)context;
    if (ctx != NULL) {
        ctx->accepted_order = goal_msg.order;
    }

    return RCLC_GOAL_ACCEPT_AND_EXECUTE;
}

static rclc_cancel_response_t cancel_callback(rcl_action_server_t* server,
                                              const rclc_goal_handle_t* goal, void* context) {
    (void)server;
    (void)context;
    printf("Cancel request for goal (uuid=%02x%02x...)\n", goal->uuid.uuid[0], goal->uuid.uuid[1]);
    return RCLC_CANCEL_ACCEPT;
}

static void accepted_callback(rcl_action_server_t* server, const rclc_goal_handle_t* goal,
                              void* context) {
    server_context_t* ctx = (server_context_t*)context;
    ctx->goal_count++;

    printf("Executing goal\n");

    // issue 0453 — compute the sequence the client ASKED for. `goal_callback`
    // stashed the accepted order in the server context; a real application
    // handling concurrent goals would key that by `goal->uuid` instead of
    // keeping one slot.
    int32_t order = ctx->accepted_order;

    // Transition to executing state
    rcl_ret_t ret = rclc_action_execute(server, goal);
    if (ret != RCL_RET_OK) {
        fprintf(stderr, "Failed to set executing state: %d\n", ret);
        return;
    }

    // Compute Fibonacci sequence with feedback
    example_interfaces__action__Fibonacci_Feedback fb;
    example_interfaces__action__Fibonacci_Feedback__Storage fb_storage;
    example_interfaces__action__Fibonacci_Feedback__init_in(&fb, &fb_storage);

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

        // Publish feedback using generated serialize
        uint8_t fb_buf[512];
        size_t fb_len = 0;
        int32_t fb_len_rc = example_interfaces__action__Fibonacci_Feedback__serialize(
            &fb, fb_buf, sizeof(fb_buf), &fb_len);
        if (fb_len_rc == 0) {
            ret = rclc_action_publish_feedback_raw(server, goal, fb_buf, fb_len);
            if (ret != RCL_RET_OK) {
                fprintf(stderr, "Failed to publish feedback: %d\n", ret);
            } else {
                printf("Publish feedback\n");
            }
        }
    }

    // Send result — copy feedback sequence to result
    example_interfaces__action__Fibonacci_Result result;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    example_interfaces__action__Fibonacci_Result__init_in(&result, &result_storage);
    result.sequence.size = fb.sequence.size;
    memcpy(result.sequence.data, fb.sequence.data, fb.sequence.size * sizeof(int32_t));

    uint8_t result_buf[512];
    size_t result_len = 0;
    int32_t result_len_rc = example_interfaces__action__Fibonacci_Result__serialize(
        &result, result_buf, sizeof(result_buf), &result_len);
    if (result_len_rc == 0) {
        ret = rclc_action_succeed(server, goal, result_buf, result_len);
        if (ret != RCL_RET_OK) {
            fprintf(stderr, "Failed to send result: %d\n", ret);
        } else {
            printf("Goal succeeded\n");
        }
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    printf("nros C Action Server (Fibonacci)\n");
    printf("===================================\n");

    // Zero-initialize all static state
    memset(&app, 0, sizeof(app));

    // Build action type info using generated type name/hash
    // Sequence capacity: 4-byte CDR header + 4-byte length + 64*4-byte data = 264
    rclc_action_type_t fibonacci_type = {
        .type_name = example_interfaces__action__Fibonacci__get_type_name(),
        .type_hash = example_interfaces__action__Fibonacci__get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    printf("Support initialized\n");
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_server", "/", &app.support),
                   1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    RCLC_CHECK_RET(rclc_action_server_init_with_callbacks(
                       &app.action_server, &app.node, "/fibonacci", &fibonacci_type, goal_callback,
                       cancel_callback, accepted_callback, &app.ctx),
                   1);
    printf("Action server created: /fibonacci\n");

    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 8, NULL), 1);
    g_executor = &app.executor;
    RCLC_CHECK_RET(rclc_executor_attach_action_server(&app.executor, &app.action_server), 1);

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for action goals (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total goals handled: %d\n", app.ctx.goal_count);
    rclc_executor_fini(&app.executor);
    rcl_action_server_fini(&app.action_server, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER()
