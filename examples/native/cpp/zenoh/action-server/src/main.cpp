/// @file main.cpp
/// @brief C++ action server example - Fibonacci (callback-based)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static volatile sig_atomic_t g_running = 1;

// Shared state — the goal callback is stateless (required by freestanding
// C++14 API) so it reaches the ActionServer + counter through globals.
static nros::ActionServer<Fibonacci>* g_srv = nullptr;
static int g_goal_count = 0;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

// ----------------------------------------------------------------------------
// Goal callback — runs the Fibonacci computation inline, publishing
// feedback and completing the goal before returning.
// ----------------------------------------------------------------------------

static nros::GoalResponse on_goal(const uint8_t uuid[16], const Fibonacci::Goal& goal) {
    if (goal.order < 0 || goal.order >= 64) {
        std::printf("Goal rejected: order=%d out of range\n", goal.order);
        return nros::GoalResponse::Reject;
    }

    g_goal_count++;
    std::printf("Goal accepted: order=%d\n", goal.order);

    int32_t a = 0;
    int32_t b = 1;
    Fibonacci::Result result;

    for (int32_t i = 0; i < goal.order && i < 64; i++) {
        result.sequence.push_back(a);

        // Publish feedback periodically
        if (i > 0 && (i % 3 == 0 || i == goal.order - 1)) {
            Fibonacci::Feedback fb;
            for (uint32_t k = 0; k < result.sequence.length(); k++) {
                fb.sequence.push_back(result.sequence[k]);
            }
            g_srv->publish_feedback(uuid, fb);
        }

        int32_t next = a + b;
        a = b;
        b = next;
    }

    if (g_srv->complete_goal(uuid, result).ok()) {
        std::printf("Goal completed: [");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            if (i > 0) std::printf(", ");
            std::printf("%d", result.sequence[i]);
        }
        std::printf("]\n");
    }
    return nros::GoalResponse::AcceptAndExecute;
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

    nros::Result ret = nros::init(locator, domain_id);
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to initialize: %d\n", ret.raw());
        return 1;
    }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_server");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create node: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Node created: %s\n", node.get_name());

    nros::ActionServer<Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create action server: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Register the goal callback.
    g_srv = &srv;
    srv.set_goal_callback(on_goal);

    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nWaiting for goal requests (Ctrl+C to exit)...\n\n");

    while (g_running && nros::ok()) {
        nros::spin_once(100);
    }

    std::printf("\nShutting down...\n");
    std::printf("Total goals handled: %d\n", g_goal_count);
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
