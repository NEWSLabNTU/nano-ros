/// @file main.cpp
/// @brief C++ service server example - handles AddTwoInts requests (manual-poll)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/srv/AddTwoInts
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

    std::printf("nros C++ Service Server (AddTwoInts)\n");
    std::printf("=====================================\n");

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
    ret = nros::create_node(node, "cpp_service_server");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create node: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Node created: %s\n", node.get_name());

    // Create service server (manual-poll style)
    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    ret = node.create_service(srv, "/add_two_ints");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create service: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Set up signal handler
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nWaiting for service requests (Ctrl+C to exit)...\n\n");

    int request_count = 0;

    // Spin + poll loop
    while (g_running && nros::ok()) {
        nros::spin_once(100);

        example_interfaces::srv::AddTwoInts::Request req;
        int64_t seq_id = 0;
        while (srv.try_recv_request(req, seq_id)) {
            request_count++;

            example_interfaces::srv::AddTwoInts::Response resp;
            resp.sum = req.a + req.b;

            std::printf("Request [%d]: %lld + %lld = %lld\n", request_count,
                        static_cast<long long>(req.a), static_cast<long long>(req.b),
                        static_cast<long long>(resp.sum));

            ret = srv.send_reply(seq_id, resp);
            if (!ret.ok()) {
                std::fprintf(stderr, "Failed to send reply: %d\n", ret.raw());
            }
        }
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    std::printf("Total requests handled: %d\n", request_count);
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
