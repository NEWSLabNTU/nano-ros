/**
 * @file main.cpp
 * @brief Zephyr C++ service server (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

extern "C" {
#if defined(CONFIG_NROS_RMW_ZENOH)
#include <zpico_zephyr.h>
#elif defined(CONFIG_NROS_RMW_XRCE)
#include <xrce_zephyr.h>
#endif
}

LOG_MODULE_REGISTER(nros_cpp_service_server, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

#include "example_interfaces.hpp"

static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char **argv)
{
    (void)argc; (void)argv;
    LOG_INF("nros Zephyr C++ Service Server");

#if defined(CONFIG_NROS_RMW_ZENOH)
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { LOG_ERR("Network not ready"); return 1; }
#elif defined(CONFIG_NROS_RMW_XRCE)
    if (xrce_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { return 1; }
#endif

#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    NROS_TRY_RET(nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                            CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_TRY_RET(nros::init("", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y"
#endif

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_service_server"), 1);
    g_logger = node.get_logger();

    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    NROS_TRY_RET(node.create_service(srv, "/add_two_ints"), 1);

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
            NROS_LOG_INFO(g_logger, "Request [%d]: %lld + %lld = %lld", request_count,
                (long long)req.a, (long long)req.b, (long long)resp.sum);
            nros::Result ret = srv.send_reply(seq_id, resp);
            if (!ret.ok()) LOG_ERR("Failed to send reply: %d", ret.raw());
        }
    }
    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
