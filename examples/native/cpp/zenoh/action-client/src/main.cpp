/// @file main.cpp
/// @brief C++ action client example - Fibonacci (blocking)

#include <cstdio>
#include <cstdlib>

#define NROS_TRY_LOG(file, line, expr, ret) \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    std::printf("nros C++ Action Client (Fibonacci)\n");
    std::printf("===================================\n");

    // Get configuration from environment
    const char* locator = std::getenv("NROS_LOCATOR");
    if (!locator) {
        locator = "tcp/127.0.0.1:7447";
    }

    uint8_t domain_id = 0;
    const char* domain_str = std::getenv("ROS_DOMAIN_ID");
    if (domain_str) {
        domain_id = static_cast<uint8_t>(std::atoi(domain_str));
    }

    std::printf("Locator: %s\n", locator);
    std::printf("Domain ID: %d\n", domain_id);

    NROS_TRY_RET(nros::init(locator, domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_action_client"), 1);
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
    std::printf("\nSending goal: order=%d\n", order);

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = order;

    uint8_t goal_id[16];
    ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) {
        std::fprintf(stderr, "Goal REJECTED by server (order=%d, ret=%d)\n", order, ret.raw());
        nros::shutdown();
        return 2;
    }
    std::printf("Goal sent: order=%d [OK]\n", order);

    // Poll for feedback while waiting — drain via the new Stream<T> API
    // (Phase 84.G7) which aligns the feedback receive surface with
    // Subscription<M>::stream() / Promise<T>::wait(). `try_recv_feedback`
    // below is still supported for callers that want the bool-convertible
    // helper.
    auto& feedback = client.feedback_stream();
    for (int i = 0; i < 20; i++) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Feedback fb;
        while (feedback.try_next(fb).ok()) {
            std::printf("Feedback: sequence=[");
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
        std::printf("Result: sequence=[");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            if (i > 0) std::printf(", ");
            std::printf("%d", result.sequence[i]);
        }
        std::printf("] [OK]\n");
    } else {
        std::fprintf(stderr, "Failed to get result: %d [FAIL]\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
