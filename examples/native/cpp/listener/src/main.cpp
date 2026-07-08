/// @file main.cpp
/// @brief C++ listener example - subscribes to std_msgs/String (manual-poll)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for std_msgs/msg/String
#include "std_msgs.hpp"

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
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    (void)argc;
    (void)argv;

    std::printf("nros C++ Listener\n");
    std::printf("===================\n");

    // Phase 212.M.2 — `nros::init()` (no-arg) pulls locator + domain_id
    // from `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` at runtime.
    NROS_TRY_RET(nros::init(), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "listener"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::Subscription<std_msgs::msg::String> sub;
    NROS_TRY_RET(node.create_subscription(sub, "/chatter"), 1);

    // Set up signal handler
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    int message_count = 0;

    // Alternative: use Stream::wait_next for blocking reception
    // std_msgs::msg::String msg;
    // sub.stream().wait_next(nros::global_handle(), 1000, msg);

    // Spin + poll loop
    while (g_running && nros::ok()) {
        nros::spin_once(100);

        std_msgs::msg::String msg;
        while (sub.try_recv(msg)) {
            message_count++;
            std::printf("I heard: [%s]\n", msg.data.c_str());
        }
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    std::printf("Total messages received: %d\n", message_count);
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
