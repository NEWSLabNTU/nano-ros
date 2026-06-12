/// @file main.cpp
/// @brief C++ service client example — **callback** variant (RFC-0041 / Phase 239).
///
/// Mirrors the Future-based `service-client` example, but receives each reply
/// through a typed `void(const Response&)` handler dispatched by `spin_once`
/// (the rclcpp `async_send_request(req, cb)` analogue). Created via the
/// callback overload `node.create_client(client, name, handler)`; requests go
/// out non-blocking with `client.async_send_request(req)`.

#include <cstdio>
#include <cstdlib>

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.hpp"

// ----------------------------------------------------------------------------
// Reply state shared with the callback. The callback overload requires a
// non-capturing `void(const Response&)`, so the handler talks to file scope.
// ----------------------------------------------------------------------------

namespace {
int g_reply_count = 0;  // bumped each time the callback fires
int64_t g_last_sum = 0; // sum from the most recent reply

void on_response(const example_interfaces::srv::AddTwoInts::Response& resp) {
    g_last_sum = resp.sum;
    g_reply_count++;
    std::printf("Response (callback): sum = %lld\n", static_cast<long long>(resp.sum));
}
} // namespace

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    std::printf("nros C++ Service Client (AddTwoInts, callback)\n");
    std::printf("===============================================\n");

    // `nros::init()` (no-arg) pulls locator + domain_id from
    // `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` with a env-var fallback (Phase
    // 212.M.2 canonical pattern, as in talker/listener). The launch-aware
    // `init_with_launch_auto` is avoided here: it routes through the 3-arg
    // `init(locator, …)` overload, which skips the env-var fallback and so
    // passes a null locator to the backend → TransportError.
    (void)argc;
    (void)argv;
    NROS_TRY_RET(nros::init(), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_service_client_callback"), 1);
    std::printf("Node created: %s\n", node.get_name());

    // RFC-0041: callback-style client. The arena dispatches `on_response` at
    // `spin_once`; sends are non-blocking via `async_send_request`.
    nros::Client<example_interfaces::srv::AddTwoInts> client;
    NROS_TRY_RET(node.create_client(client, "/add_two_ints", &on_response), 1);
    std::printf("Callback service client created for: /add_two_ints\n");

    // Let discovery settle (the callback client has no Future to gate on).
    for (int i = 0; i < 20; i++) {
        nros::spin_once(50);
    }

    struct TestCase {
        int64_t a;
        int64_t b;
    };
    TestCase test_cases[] = {{5, 3}, {10, 20}, {100, 200}, {-5, 10}};
    int num_cases = static_cast<int>(sizeof(test_cases) / sizeof(test_cases[0]));

    std::printf("\nCalling service %d times (async + callback)...\n\n", num_cases);

    int success_count = 0;

    for (int i = 0; i < num_cases; i++) {
        example_interfaces::srv::AddTwoInts::Request req;
        req.a = test_cases[i].a;
        req.b = test_cases[i].b;

        int before = g_reply_count;
        std::printf("Calling service: %lld + %lld = ?\n", static_cast<long long>(req.a),
                    static_cast<long long>(req.b));

        nros::Result send = client.async_send_request(req);
        if (!send.ok()) {
            std::fprintf(stderr, "Call [%d]: async send failed with error %d\n", i + 1, send.raw());
            continue;
        }

        // Spin until the reply callback fires (or a 5 s budget elapses).
        int waited_ms = 0;
        while (g_reply_count == before && waited_ms < 5000) {
            nros::spin_once(50);
            waited_ms += 50;
        }

        if (g_reply_count > before) {
            int64_t expected = req.a + req.b;
            if (g_last_sum == expected) {
                std::printf("Call [%d]: OK (sum = %lld)\n", i + 1,
                            static_cast<long long>(g_last_sum));
                success_count++;
            } else {
                std::printf("Call [%d]: MISMATCH (got %lld, expected %lld)\n", i + 1,
                            static_cast<long long>(g_last_sum), static_cast<long long>(expected));
            }
        } else {
            std::fprintf(stderr, "Call [%d]: timeout waiting for callback\n", i + 1);
        }
    }

    std::printf("\n%d/%d callback calls succeeded\n", success_count, num_cases);

    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return (success_count == num_cases) ? 0 : 1;
}
