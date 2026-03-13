/// @file main.cpp
/// @brief C++ service client example - calls AddTwoInts service (blocking)

#include <cstdio>
#include <cstdlib>

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    std::printf("nros C++ Service Client (AddTwoInts)\n");
    std::printf("=====================================\n");

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
    ret = nros::create_node(node, "cpp_service_client");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create node: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }
    std::printf("Node created: %s\n", node.get_name());

    // Create service client
    nros::Client<example_interfaces::srv::AddTwoInts> client;
    ret = node.create_client(client, "/add_two_ints");
    if (!ret.ok()) {
        std::fprintf(stderr, "Failed to create client: %d\n", ret.raw());
        nros::shutdown();
        return 1;
    }

    // Test cases
    struct TestCase {
        int64_t a;
        int64_t b;
    };

    TestCase test_cases[] = {{5, 3}, {10, 20}, {100, 200}, {-5, 10}};
    int num_cases = static_cast<int>(sizeof(test_cases) / sizeof(test_cases[0]));

    std::printf("\nCalling service %d times...\n\n", num_cases);

    int success_count = 0;

    for (int i = 0; i < num_cases; i++) {
        example_interfaces::srv::AddTwoInts::Request req;
        req.a = test_cases[i].a;
        req.b = test_cases[i].b;

        example_interfaces::srv::AddTwoInts::Response resp;
        ret = client.call(req, resp);

        if (ret.ok()) {
            std::printf("Call [%d]: %lld + %lld = %lld", i + 1,
                        static_cast<long long>(req.a), static_cast<long long>(req.b),
                        static_cast<long long>(resp.sum));

            if (resp.sum == req.a + req.b) {
                std::printf(" [OK]\n");
                success_count++;
            } else {
                std::printf(" [MISMATCH: expected %lld]\n",
                            static_cast<long long>(req.a + req.b));
            }
        } else {
            std::fprintf(stderr, "Call [%d]: Failed with error %d\n", i + 1, ret.raw());
        }
    }

    std::printf("\n%d/%d calls succeeded\n", success_count, num_cases);

    // Cleanup
    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return (success_count == num_cases) ? 0 : 1;
}
