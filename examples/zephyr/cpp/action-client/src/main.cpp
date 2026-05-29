/**
 * @file main.cpp
 * @brief Zephyr C++ action client (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

extern "C" {
#include <nros/platform_zephyr.h>
}

LOG_MODULE_REGISTER(nros_cpp_action_client, LOG_LEVEL_INF);

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
    LOG_INF("nros Zephyr C++ Action Client");

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
                            CONFIG_NROS_DOMAIN_ID, "zephyr_cpp_action_client"),
                 1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_TRY_RET(nros::init("", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y"
#endif

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_action_client"), 1);
    g_logger = node.get_logger();

    using Fibonacci = example_interfaces::action::Fibonacci;
    nros::ActionClient<Fibonacci> client;
    NROS_TRY_RET(node.create_action_client(client, "/fibonacci"), 1);

    for (int i = 0; i < 30; i++)
        nros::spin_once(100);

    int32_t order = 10;
    LOG_INF("Sending goal: order=%d", order);
    Fibonacci::Goal goal;
    goal.order = order;
    uint8_t goal_id[16];
    nros::Result ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) {
        LOG_ERR("Failed to send goal: %d", ret.raw());
        nros::shutdown();
        return 1;
    }
    NROS_LOG_INFO(g_logger, "Goal sent: order=%d", order);

    for (int i = 0; i < 30; i++) {
        nros::spin_once(100);
        Fibonacci::Feedback fb;
        while (client.try_recv_feedback(fb))
            NROS_LOG_INFO(g_logger, "Feedback: length=%d", fb.sequence.length());
    }

    auto result_fut = client.get_result_future(goal_id);
    Fibonacci::Result result;
    ret = result_fut.wait(nros::global_handle(), 10000, result);
    if (ret.ok())
        LOG_INF("Result received: length=%d [OK]", result.sequence.length());
    else
        LOG_ERR("Failed to get result: %d", ret.raw());

    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
