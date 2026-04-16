/// @file main.cpp
/// @brief C++ action client example - Fibonacci (Future-based)

#include <cstdio>
#include <cstdlib>

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    std::printf("nros C++ Action Client (Fibonacci)\n");
    std::printf("===================================\n");

    // Get configuration from environment
    const char* locator = std::getenv("ZENOH_LOCATOR");
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

    // Initialize nros session
    nros::Result ret = nros::init(locator, domain_id);
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to initialize: %d\n", ret.raw());
        return 1;
    }

    // Create node
    nros::Node node;
    ret = nros::create_node(node, "cpp_action_client");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create node: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Node created: %s\n", node.get_name());

    // Create action client
    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    ret = node.create_action_client(client, "/fibonacci");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create action client: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Test with order=10
    int32_t order = 10;
    std::printf("\nSending goal: order=%d\n", order);

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = order;

    auto goal_fut = client.send_goal_future(goal);
    if (goal_fut.is_consumed()) {
        std::fprintf(stderr, "Failed to send goal\n");
        nros::shutdown();
        return 1;
    }

    nros::ActionClient<example_interfaces::action::Fibonacci>::GoalAccept accept;
    ret = goal_fut.wait(nros::global_handle(), 10000, accept);
    if (!ret.ok() || !accept.accepted) {
        std::fprintf(stderr, "Goal not accepted: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Goal accepted [OK]\n");

    // Poll for feedback while waiting
    for (int i = 0; i < 20; i++) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Feedback fb;
        while (client.try_recv_feedback(fb)) {
            std::printf("Feedback: sequence=[");
            for (uint32_t k = 0; k < fb.sequence.length(); k++) {
                if (k > 0) std::printf(", ");
                std::printf("%d", fb.sequence[k]);
            }
            std::printf("]\n");
        }
    }

    // Get result (Future-based)
    auto result_fut = client.get_result_future(accept.goal_id);
    if (result_fut.is_consumed()) {
        std::fprintf(stderr, "Failed to request result\n");
        nros::shutdown();
        return 1;
    }

    example_interfaces::action::Fibonacci::Result result;
    ret = result_fut.wait(nros::global_handle(), 30000, result);
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
