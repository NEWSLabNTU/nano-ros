/// @file main.cpp
/// @brief C++ action server example - Fibonacci (callback-based)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#define NROS_TRY_LOG(file, line, expr, ret) \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/action/Fibonacci
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static volatile sig_atomic_t g_running = 1;

/// State that the goal callback needs to reach — held on the stack of
/// `main` and handed to the ActionServer via the Phase 84.G9
/// `set_goal_callback_with_ctx` overload. The older `set_goal_callback`
/// path requires a stateless function pointer and forces file-scope
/// globals; `_with_ctx` lets us pass a `void*` through each invocation
/// so the callback reaches the server and counter without any globals.
struct ServerState {
    nros::ActionServer<Fibonacci>* srv;
    int goal_count;
};

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

// ----------------------------------------------------------------------------
// Goal callback — runs the Fibonacci computation inline, publishing
// feedback and completing the goal before returning. Reaches the
// ActionServer + counter through the `void* ctx` parameter (Phase 84.G9).
// ----------------------------------------------------------------------------

static nros::GoalResponse on_goal(const uint8_t uuid[16], const Fibonacci::Goal& goal,
                                  void* ctx) {
    auto* state = static_cast<ServerState*>(ctx);
    if (goal.order < 0 || goal.order >= 64) {
        std::printf("Goal rejected: order=%d out of range\n", goal.order);
        return nros::GoalResponse::Reject;
    }

    state->goal_count++;
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
            state->srv->publish_feedback(uuid, fb);
        }

        int32_t next = a + b;
        a = b;
        b = next;
    }

    if (state->srv->complete_goal(uuid, result).ok()) {
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

int nros_app_main(int argc, char **argv) {
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

    NROS_TRY_RET(nros::init(locator, domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_action_server"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::ActionServer<Fibonacci> srv;
    NROS_TRY_RET(node.create_action_server(srv, "/fibonacci"), 1);

    // Register the goal callback with a ServerState context (Phase 84.G9) —
    // no globals needed.
    ServerState state{&srv, 0};
    srv.set_goal_callback_with_ctx(on_goal, &state);

    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nWaiting for goal requests (Ctrl+C to exit)...\n\n");

    while (g_running && nros::ok()) {
        nros::spin_once(100);
    }

    std::printf("\nShutting down...\n");
    std::printf("Total goals handled: %d\n", state.goal_count);
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
