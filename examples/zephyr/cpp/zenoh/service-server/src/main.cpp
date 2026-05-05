/**
 * @file main.cpp
 * @brief Zephyr C++ service server example using nros-cpp API
 *
 * This example demonstrates handling AddTwoInts service requests on Zephyr RTOS
 * using the nros C++ API (nros::init, nros::Node, nros::Service<S>).
 * Uses manual-poll with spin_once() + try_recv_request().
 * The nros module handles zenoh initialization and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_cpp_service_server, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

extern "C" {
#include <zpico_zephyr.h>
}

#include <nros/app_main.h>
#include <nros/nros.hpp>

// Generated C++ service bindings
#include "example_interfaces.hpp"

/* ============================================================================
 * Application
 * ============================================================================ */

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    LOG_INF("nros Zephyr C++ Service Server");
    LOG_INF("================================");

    /* Wait for network interface */
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_service_server"), 1);

    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    NROS_TRY_RET(node.create_service(srv, "/add_two_ints"), 1);
    nros::Result ret;

    /* Spin + poll loop */
    LOG_INF("Waiting for service requests...");

    int request_count = 0;

    while (true) {
        nros::spin_once(100);

        example_interfaces::srv::AddTwoInts::Request req;
        int64_t seq_id = 0;
        while (srv.try_recv_request(req, seq_id)) {
            request_count++;

            example_interfaces::srv::AddTwoInts::Response resp;
            resp.sum = req.a + req.b;

            LOG_INF("Request [%d]: %lld + %lld = %lld", request_count,
                    static_cast<long long>(req.a), static_cast<long long>(req.b),
                    static_cast<long long>(resp.sum));

            ret = srv.send_reply(seq_id, resp);
            if (!ret.ok()) {
                LOG_ERR("Failed to send reply: %d", ret.raw());
            }
        }
    }

    /* Cleanup (unreachable in this example) */
    nros::shutdown();

    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
