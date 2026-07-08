/// @file main.cpp
/// @brief C++ action client example - Fibonacci (blocking)

#include <cstdio>
#include <cstdlib>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    std::printf("nros C++ Action Client (Fibonacci)\n");
    std::printf("===================================\n");

    // Launch-aware init. Env overlay active today.
    NROS_TRY_RET(nros::init_with_launch_auto(argc, argv), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "fibonacci_action_client"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    NROS_TRY_RET(node.create_action_client(client, "/fibonacci"), 1);
    nros::Result ret;

    // Default order=10; override via NROS_TEST_GOAL_ORDER for tests that
    // want to exercise server-side rejection (order >= 64) or other edges.
    int32_t order = 10;
    if (const char* ord = std::getenv("NROS_TEST_GOAL_ORDER")) {
        order = std::atoi(ord);
    }
    std::printf("\nSending goal\n");

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = order;

    uint8_t goal_id[16];
    ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) {
        std::fprintf(stderr, "Goal was rejected by server (order=%d, ret=%d)\n", order, ret.raw());
        nros::shutdown();
        return 2;
    }
    std::printf("Goal accepted by server, waiting for result\n");

    // Poll for feedback while waiting — drain via the Stream<T> API,
    // which aligns the feedback receive surface with
    // Subscription<M>::stream() / Promise<T>::wait(). `try_recv_feedback`
    // below is still supported for callers that want the bool-convertible
    // helper.
    auto& feedback = client.feedback_stream();
    for (int i = 0; i < 20; i++) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Feedback fb;
        while (feedback.try_next(fb).ok()) {
            std::printf("Next number in sequence received: [");
            for (uint32_t k = 0; k < fb.sequence.length(); k++) {
                if (k > 0) std::printf(", ");
                std::printf("%d", fb.sequence[k]);
            }
            std::printf("]\n");
        }
    }

    // Get result (blocking)
    example_interfaces::action::Fibonacci::Result result;
    ret = client.get_result(goal_id, result);
    if (ret.ok()) {
        std::printf("Result received: [");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            if (i > 0) std::printf(", ");
            std::printf("%d", result.sequence[i]);
        }
        std::printf("]\n");
    } else {
        std::fprintf(stderr, "Failed to get result: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
