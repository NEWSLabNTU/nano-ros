/// @file main.cpp
/// @brief C++ service server example - handles AddTwoInts requests (manual-poll)

#include <cstdio>
#include <cstdlib>
#include <csignal>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

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
    std::printf("nros C++ Service Server (AddTwoInts)\n");
    std::printf("=====================================\n");

    // Launch-aware init. Env overlay is the active source today
    // (`$NROS_LOCATOR` / `$ROS_DOMAIN_ID`).
    NROS_TRY_RET(nros::init_with_launch_auto(argc, argv), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "add_two_ints_server"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    NROS_TRY_RET(node.create_service(srv, "/add_two_ints"), 1);
    nros::Result ret;

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

            std::printf("Incoming request\na: %lld b: %lld\n", static_cast<long long>(req.a),
                        static_cast<long long>(req.b));

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
