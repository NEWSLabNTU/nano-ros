/// @file main.cpp
/// @brief C++ listener example - subscribes to std_msgs/Int32 (manual-poll)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#include <nros/nros.hpp>

// Generated C++ bindings for std_msgs/msg/Int32
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
    (void)argc;
    (void)argv;

    std::printf("nros C++ Listener\n");
    std::printf("===================\n");

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

    // Initialize nros session
    nros::Result ret = nros::init(locator, domain_id);
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to initialize: %d\n", ret.raw());
        return 1;
    }

    // Create node
    nros::Node node;
    ret = nros::create_node(node, "cpp_listener");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create node: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Node created: %s\n", node.get_name());

    // Create subscription (manual-poll style)
    nros::Subscription<std_msgs::msg::Int32> sub;
    ret = node.create_subscription(sub, "/chatter");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create subscription: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Set up signal handler
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nWaiting for messages (Ctrl+C to exit)...\n\n");

    int message_count = 0;

    // Alternative: use Stream::wait_next for blocking reception
    // std_msgs::msg::Int32 msg;
    // sub.stream().wait_next(nros::global_handle(), 1000, msg);

    // Spin + poll loop
    while (g_running && nros::ok()) {
        nros::spin_once(100);

        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            message_count++;
            std::printf("Received: %d\n", msg.data);
        }
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    std::printf("Total messages received: %d\n", message_count);
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
