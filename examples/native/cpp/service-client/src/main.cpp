/// @file main.cpp
/// @brief C++ service client example - calls AddTwoInts service (async Future)

#include <cstdio>
#include <cstdlib>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
    std::setvbuf(stdout, nullptr, _IOLBF, 0);
    std::printf("nros C++ Service Client (AddTwoInts)\n");
    std::printf("=====================================\n");

    // Launch-aware init. Falls back to the env overlay
    // (`$NROS_LOCATOR` / `$ROS_DOMAIN_ID` / `$RMW_IMPLEMENTATION`) when
    // no `$NROS_RUNTIME_OVERLAY` / launch XML is in scope.
    NROS_TRY_RET(nros::init_with_launch_auto(argc, argv), 1);

    // Operands from the first two positional args (default: 2 3).
    int64_t a = 2;
    int64_t b = 3;
    if (argc >= 3) {
        char* end_a = nullptr;
        char* end_b = nullptr;
        long long parsed_a = std::strtoll(argv[1], &end_a, 10);
        long long parsed_b = std::strtoll(argv[2], &end_b, 10);
        if (end_a && *end_a == '\0' && end_b && *end_b == '\0') {
            a = static_cast<int64_t>(parsed_a);
            b = static_cast<int64_t>(parsed_b);
        }
    }

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "add_two_ints_client"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::Client<example_interfaces::srv::AddTwoInts> client;
    NROS_TRY_RET(node.create_client(client, "/add_two_ints"), 1);

    example_interfaces::srv::AddTwoInts::Request req;
    req.a = a;
    req.b = b;

    example_interfaces::srv::AddTwoInts::Response resp;
    auto fut = client.send_request(req);
    if (fut.is_consumed()) {
        std::fprintf(stderr, "send_request failed\n");
        nros::shutdown();
        return 1;
    }
    nros::Result ret = fut.wait(nros::global_handle(), 5000, resp);

    int exit_code = 0;
    if (ret.ok()) {
        std::printf("Result of add_two_ints: %lld\n", static_cast<long long>(resp.sum));
    } else {
        std::fprintf(stderr, "Service call failed with error %d\n", ret.raw());
        exit_code = 1;
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return exit_code;
}
