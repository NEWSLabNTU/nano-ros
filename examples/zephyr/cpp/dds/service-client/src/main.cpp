/**
 * @file main.cpp
 * @brief Zephyr C++ service client example using nros-cpp API
 *
 * This example demonstrates calling an AddTwoInts service on Zephyr RTOS
 * using the nros C++ API (nros::init, nros::Node, nros::Client<S>).
 * Uses async send_request() + Future::wait() with sleep between calls.
 * The nros module handles zenoh initialization and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_cpp_dds_service_client, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ service bindings
#include "example_interfaces.hpp"

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C++ Service Client");
    LOG_INF("================================");

    NROS_TRY_RET(nros::init("", CONFIG_NROS_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_service_client"), 1);

    nros::Client<example_interfaces::srv::AddTwoInts> client;
    NROS_TRY_RET(node.create_client(client, "/add_two_ints"), 1);
    nros::Result ret;

    /* Allow time for connection to stabilize */
    k_sleep(K_SECONDS(2));

    /* Test cases */
    struct TestCase {
        int64_t a;
        int64_t b;
    };

    TestCase test_cases[] = {{5, 3}, {10, 20}, {100, 200}, {-5, 10}};
    int num_cases = static_cast<int>(sizeof(test_cases) / sizeof(test_cases[0]));

    LOG_INF("Calling service %d times...", num_cases);

    int success_count = 0;

    for (int i = 0; i < num_cases; i++) {
        example_interfaces::srv::AddTwoInts::Request req;
        req.a = test_cases[i].a;
        req.b = test_cases[i].b;

        example_interfaces::srv::AddTwoInts::Response resp;
        auto fut = client.send_request(req);
        if (fut.is_consumed()) {
            LOG_ERR("Call [%d]: send_request failed", i + 1);
            continue;
        }
        ret = fut.wait(nros::global_handle(), 5000, resp);

        if (ret.ok()) {
            if (resp.sum == req.a + req.b) {
                LOG_INF("Call [%d]: %lld + %lld = %lld [OK]", i + 1,
                        static_cast<long long>(req.a), static_cast<long long>(req.b),
                        static_cast<long long>(resp.sum));
                success_count++;
            } else {
                LOG_ERR("Call [%d]: mismatch %lld + %lld = %lld (expected %lld)", i + 1,
                        static_cast<long long>(req.a), static_cast<long long>(req.b),
                        static_cast<long long>(resp.sum),
                        static_cast<long long>(req.a + req.b));
            }
        } else {
            LOG_ERR("Call [%d]: failed with error %d", i + 1, ret.raw());
        }

        k_sleep(K_SECONDS(1));
    }

    LOG_INF("%d/%d calls succeeded", success_count, num_cases);

    /* Cleanup */
    nros::shutdown();

    return 0;
}
