/// @file main.c
/// @brief C action client example - sends Fibonacci goal, receives feedback

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// The C API at rcl/rclc's include paths
#include <rclc/action_client.h>
#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>

// Generated C bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_action_client_t action_client;
    rclc_executor_t executor;
} app;

static volatile sig_atomic_t g_running = 1;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

// ----------------------------------------------------------------------------
// Helper: print a Fibonacci sequence
// ----------------------------------------------------------------------------

static void print_sequence(const example_interfaces__action__Fibonacci_Feedback* fb) {
    printf("[");
    for (uint32_t i = 0; i < fb->sequence.size; i++) {
        if (i > 0) printf(", ");
        printf("%d", fb->sequence.data[i]);
    }
    printf("]");
}

// ----------------------------------------------------------------------------
// Feedback callback
// ----------------------------------------------------------------------------

static void feedback_callback(const rclc_goal_uuid_t* goal_uuid, const uint8_t* feedback,
                              size_t feedback_len, void* context) {
    (void)goal_uuid;
    (void)context;

    example_interfaces__action__Fibonacci_Feedback fb;
    example_interfaces__action__Fibonacci_Feedback__Storage fb_storage;
    example_interfaces__action__Fibonacci_Feedback__init_in(&fb, &fb_storage);
    if (example_interfaces__action__Fibonacci_Feedback__deserialize(&fb, feedback, feedback_len) ==
        0) {
        printf("Next number in sequence received: ");
        print_sequence(&fb);
        printf("\n");
    } else {
        fprintf(stderr, "Failed to deserialize feedback\n");
    }
}

// ----------------------------------------------------------------------------
// Result callback — receipt is noted here; the terminal `Result received:`
// line is printed by the blocking get_result round-trip in main.
// ----------------------------------------------------------------------------

static int g_result_received = 0;

static void result_callback(const rclc_goal_uuid_t* goal_uuid, rclc_goal_status_t status,
                            const uint8_t* result, size_t result_len, void* context) {
    (void)goal_uuid;
    (void)status;
    (void)result;
    (void)result_len;
    (void)context;

    g_result_received = 1;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    printf("nros C Action Client (Fibonacci)\n");
    printf("===================================\n");

    // Zero-initialize all static state
    memset(&app, 0, sizeof(app));

    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    printf("Support initialized\n");
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_client", "/", &app.support),
                   1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    RCLC_CHECK_RET(rcl_action_client_init(
                       &app.action_client, &app.node,
                       ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci),
                       "/fibonacci", NULL),
                   1);
    printf("Action client created: /fibonacci\n");

    RCLC_SOFTCHECK(
        rclc_action_client_set_feedback_callback(&app.action_client, feedback_callback, NULL));
    RCLC_SOFTCHECK(
        rclc_action_client_set_result_callback(&app.action_client, result_callback, NULL));

    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_attach_action_client(&app.executor, &app.action_client), 1);
    rcl_ret_t ret = RCL_RET_OK;

    // Warm-up: spin to allow Zenoh to discover the server's queryables
    for (int i = 0; i < 300; i++) {
        rclc_executor_spin_some(&app.executor, 10000000ULL); // 10ms
    }

    // Send goal: compute Fibonacci sequence of order 10
    example_interfaces__action__Fibonacci_Goal goal;
    memset(&goal, 0, sizeof(goal));
    goal.order = 10;

    uint8_t goal_buf[64];
    size_t goal_len = 0;
    int32_t goal_len_rc = example_interfaces__action__Fibonacci_Goal__serialize(
        &goal, goal_buf, sizeof(goal_buf), &goal_len);
    if (goal_len_rc != 0) {
        fprintf(stderr, "Failed to serialize goal\n");
        goto cleanup;
    }

    // phase-338 W8 — wait for the action server, then send ONCE.
    //
    // Issue 0153 / #188 root cause: on zenoh the server's readiness gossips
    // ahead of its send-goal queryable ROUTE, and a `send_goal` query fired in
    // that window matches no queryable and can only time out — a zenoh get is
    // evaluated against the queryables visible at fire time, so waiting longer
    // on the same query never helps.
    //
    // The NuttX copy used to retry the send three times with a 1 s spin
    // between attempts. `wait_for_action_server` is the primitive that was
    // approximating: it probes the send-goal queryable's liveliness and
    // RE-probes until the budget expires, so a server that appears mid-wait is
    // still seen. Same shape as `rclcpp_action::Client::wait_for_action_server`.
    ret = rclc_action_client_wait_for_action_server(&app.action_client, &app.executor, 10000);
    // issue 1686 — RCL_RET_UNSUPPORTED: this backend cannot see servers at
    // all (XRCE: the Agent owns the DDS graph), so the wait answered at once.
    // Send anyway; the request's own timeout is then the probe.
    if (ret != RCL_RET_OK && ret != RCL_RET_UNSUPPORTED) {
        fprintf(stderr, "Action server did not appear within 10s: %d\n", ret);
        fprintf(stderr, "(Is the action server running?)\n");
        goto cleanup;
    }

    printf("\nSending goal\n");

    rclc_goal_uuid_t goal_uuid;
    ret = rclc_action_send_goal(&app.action_client, &app.executor, goal_buf, goal_len, &goal_uuid);

    if (ret != RCL_RET_OK) {
        // Issue 0868 — the C++ sibling printed "Goal was rejected by server"
        // for every non-OK code; this one was only vague. Both are fixable
        // now that `rclc_action_send_goal` distinguishes REJECTED (the server
        // decided) from TIMEOUT (no answer) from ERROR (never sent).
        if (ret == RCLC_RET_REJECTED) {
            fprintf(stderr, "Goal was rejected by server\n");
        } else if (ret == RCL_RET_TIMEOUT) {
            fprintf(stderr, "No goal response from server: %d\n", ret);
            fprintf(stderr, "(Is the action server running?)\n");
        } else {
            fprintf(stderr, "Failed to send goal: %d\n", ret);
        }
        goto cleanup;
    }

    printf("Goal accepted by server, waiting for result\n");

    // Wait for result with timeout
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    // Poll for result using get_result (blocking)
    rclc_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = rclc_action_get_result(&app.action_client, &app.executor, &goal_uuid, &final_status,
                                 result_buf, sizeof(result_buf), &result_len);

    if (ret == RCL_RET_OK) {
        example_interfaces__action__Fibonacci_Result result;
        example_interfaces__action__Fibonacci_Result__Storage result_storage;
        example_interfaces__action__Fibonacci_Result__init_in(&result, &result_storage);
        if (example_interfaces__action__Fibonacci_Result__deserialize(&result, result_buf,
                                                                      result_len) == 0) {
            printf("Result received: [");
            for (uint32_t i = 0; i < result.sequence.size; i++) {
                if (i > 0) printf(", ");
                printf("%d", result.sequence.data[i]);
            }
            printf("]\n");
        } else {
            fprintf(stderr, "Failed to deserialize result\n");
        }
    } else if (ret == RCL_RET_TIMEOUT) {
        fprintf(stderr, "Timeout waiting for result\n");
    } else {
        fprintf(stderr, "Failed to get result: %d\n", ret);
    }

cleanup:
    // Cleanup
    printf("\nShutting down...\n");
    rclc_executor_fini(&app.executor);
    rcl_action_client_fini(&app.action_client, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return (ret == RCL_RET_OK) ? 0 : 1;
}
