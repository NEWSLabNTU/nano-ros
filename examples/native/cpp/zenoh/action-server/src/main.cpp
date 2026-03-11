/// @file main.cpp
/// @brief C++ action server example - Fibonacci (manual-poll)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static volatile sig_atomic_t g_running = 1;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    std::printf("nros C++ Action Server (Fibonacci)\n");
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
    ret = nros::create_node(node, "cpp_action_server");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create node: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Node created: %s\n", node.get_name());

    // Create action server (manual-poll style)
    nros::ActionServer<example_interfaces::action::Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create action server: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Set up signal handler
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nWaiting for goal requests (Ctrl+C to exit)...\n\n");

    int goal_count = 0;

    // Spin + poll loop
    while (g_running && nros::ok()) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Goal goal;
        uint8_t goal_id[16];
        while (srv.try_recv_goal(goal, goal_id)) {
            goal_count++;
            std::printf("Goal received: order=%d\n", goal.order);

            // Compute Fibonacci sequence with feedback
            int32_t a = 0;
            int32_t b = 1;

            example_interfaces::action::Fibonacci::Result result;

            for (int32_t i = 0; i < goal.order && i < 64; i++) {
                result.sequence.push_back(a);

                // Publish feedback periodically
                if (i > 0 && (i % 3 == 0 || i == goal.order - 1)) {
                    example_interfaces::action::Fibonacci::Feedback fb;
                    for (uint32_t k = 0; k < result.sequence.length(); k++) {
                        fb.sequence.push_back(result.sequence[k]);
                    }
                    srv.publish_feedback(goal_id, fb);
                }

                int32_t next = a + b;
                a = b;
                b = next;
            }

            // Complete goal
            ret = srv.complete_goal(goal_id, result);
            if (ret.ok()) {
                std::printf("Goal completed: [");
                for (uint32_t i = 0; i < result.sequence.length(); i++) {
                    if (i > 0) std::printf(", ");
                    std::printf("%d", result.sequence[i]);
                }
                std::printf("]\n");
            } else {
                std::fprintf(stderr, "Failed to complete goal: %d\n", ret.raw());
            }
        }
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    std::printf("Total goals handled: %d\n", goal_count);
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
