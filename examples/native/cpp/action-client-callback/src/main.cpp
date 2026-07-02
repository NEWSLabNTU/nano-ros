/// @file main.cpp
/// @brief C++ action client example — **callback** variant.
///
/// Mirrors the Future/stream `action-client` example, but receives goal
/// acceptance, feedback, and the result through `SendGoalOptions` callbacks
/// dispatched by `ActionClient::poll()` at each `spin_once` (the rclcpp
/// `send_goal(goal, SendGoalOptions{...})` analogue). Drives Fibonacci.

#include <cstdio>
#include <cstdlib>
#include <cstring>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Callback state. SendGoalOptions takes plain function pointers, so the
// handlers talk to file scope rather than capturing.
// ----------------------------------------------------------------------------

namespace {
int g_accepted = -1; // -1 pending, 0 rejected, 1 accepted
int g_feedback_count = 0;
int g_result_status = -1; // -1 pending, else GoalStatus
int g_result_len = 0;
uint8_t g_goal_id[16] = {0};

void on_goal_response(bool accepted, const uint8_t goal_id[16], void* ctx) {
    (void)ctx;
    (void)goal_id;
    g_accepted = accepted ? 1 : 0;
    if (accepted) {
        std::printf("Goal accepted by server, waiting for result\n");
    }
}

void on_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) {
    (void)ctx;
    (void)goal_id;
    g_feedback_count++;
    example_interfaces::action::Fibonacci::Feedback fb;
    if (example_interfaces::action::Fibonacci::Feedback::ffi_deserialize(data, len, &fb) == 0) {
        std::printf("Next number in sequence received: [");
        for (uint32_t i = 0; i < fb.sequence.length(); i++) {
            if (i > 0) std::printf(", ");
            std::printf("%d", fb.sequence[i]);
        }
        std::printf("]\n");
    } else {
        std::fprintf(stderr, "Failed to deserialize feedback (%zu bytes)\n", len);
    }
}

void on_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len,
               void* ctx) {
    (void)ctx;
    (void)goal_id;
    g_result_status = status;
    g_result_len = static_cast<int>(len);
    example_interfaces::action::Fibonacci::Result result;
    if (example_interfaces::action::Fibonacci::Result::ffi_deserialize(data, len, &result) == 0) {
        std::printf("Result received: [");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            if (i > 0) std::printf(", ");
            std::printf("%d", result.sequence[i]);
        }
        std::printf("]\n");
    } else {
        std::fprintf(stderr, "Failed to deserialize result (%zu bytes)\n", len);
    }
}
} // namespace

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    std::printf("nros C++ Action Client (Fibonacci, callback)\n");
    std::printf("=============================================\n");

    // Launch-aware init. Env overlay
    // (`$NROS_LOCATOR` / `$ROS_DOMAIN_ID`) active today.
    NROS_TRY_RET(nros::init_with_launch_auto(argc, argv), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "fibonacci_action_client_cb"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    NROS_TRY_RET(node.create_action_client(client, "/fibonacci"), 1);
    std::printf("Callback action client created for: /fibonacci\n");

    // Register the three callbacks before sending the goal.
    nros::ActionClient<example_interfaces::action::Fibonacci>::SendGoalOptions options;
    options.goal_response = &on_goal_response;
    options.feedback = &on_feedback;
    options.result = &on_result;
    options.context = nullptr;
    NROS_TRY_RET(client.set_callbacks(options), 1);

    // Let discovery settle.
    for (int i = 0; i < 20; i++) {
        nros::spin_once(50);
        client.poll();
    }

    int32_t order = 10;
    if (const char* ord = std::getenv("NROS_TEST_GOAL_ORDER")) {
        order = std::atoi(ord);
    }
    std::printf("\nSending goal\n");

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = order;
    if (!client.send_goal_async(goal, g_goal_id).ok()) {
        std::fprintf(stderr, "send_goal_async failed\n");
        nros::shutdown();
        return 1;
    }

    // Drive the executor: poll() dispatches goal-response → feedback → result.
    // 1) Spin until the goal is accepted (or rejected).
    for (int i = 0; i < 100 && g_accepted < 0; i++) {
        nros::spin_once(100);
        client.poll();
    }
    if (g_accepted == 0) {
        std::fprintf(stderr, "Goal rejected\n");
        nros::shutdown();
        return 2;
    }
    if (g_accepted < 0) {
        std::fprintf(stderr, "No goal response\n");
        nros::shutdown();
        return 1;
    }

    // 2) Drain feedback while the server executes (Fibonacci streams partials),
    //    mirroring the stock client's feedback window before the result.
    for (int i = 0; i < 30; i++) {
        nros::spin_once(100);
        client.poll();
    }

    // 3) Request the result; spin until the result callback fires.
    if (!client.get_result_async(g_goal_id).ok()) {
        std::fprintf(stderr, "get_result_async failed\n");
    }
    for (int i = 0; i < 100 && g_result_status < 0; i++) {
        nros::spin_once(100);
        client.poll();
    }

    int rc;
    if (g_result_status >= 0) {
        rc = 0;
    } else {
        std::fprintf(stderr, "Timed out waiting for result callback\n");
        rc = 1;
    }

    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return rc;
}
