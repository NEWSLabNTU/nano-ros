/**
 * @file main.cpp
 * @brief Zephyr C++ service client (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

extern "C" {
#include <nros/platform_zephyr.h>
}

LOG_MODULE_REGISTER(nros_cpp_service_client, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

#include "example_interfaces.hpp"

static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    LOG_INF("nros Zephyr C++ Service Client");

    if (nros_platform_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    /* Distinct XRCE session name (Phase 177.9.F) — the 2-arg nros::init
     * defaults the client key to "nros_cpp", colliding with the peer on
     * the same Agent (the Agent resets the shared client); use this
     * process's node name. */
    NROS_TRY_RET(nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                            CONFIG_NROS_DOMAIN_ID, "zephyr_cpp_service_client"),
                 1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_TRY_RET(nros::init("", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y"
#endif

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_service_client"), 1);
    g_logger = node.get_logger();

    nros::Client<example_interfaces::srv::AddTwoInts> client;
    NROS_TRY_RET(node.create_client(client, "/add_two_ints"), 1);

    k_sleep(K_SECONDS(2));

    struct TestCase {
        int64_t a;
        int64_t b;
    };
    TestCase test_cases[] = {{5, 3}, {10, 20}, {100, 200}, {-5, 10}};
    int num_cases = (int)(sizeof(test_cases) / sizeof(test_cases[0]));

    LOG_INF("Calling service %d times...", num_cases);
    int success_count = 0;
    for (int i = 0; i < num_cases; i++) {
        example_interfaces::srv::AddTwoInts::Request req;
        req.a = test_cases[i].a;
        req.b = test_cases[i].b;
        example_interfaces::srv::AddTwoInts::Response resp;
        auto fut = client.send_request(req);
        if (fut.is_consumed()) continue;
        nros::Result ret = fut.wait(nros::global_handle(), 5000, resp);
        if (ret.ok() && resp.sum == req.a + req.b) {
            NROS_LOG_INFO(g_logger, "Call [%d]: %lld + %lld = %lld [OK]", i + 1, (long long)req.a,
                          (long long)req.b, (long long)resp.sum);
            success_count++;
        }
        k_sleep(K_SECONDS(1));
    }

    LOG_INF("%d/%d calls succeeded", success_count, num_cases);
    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
