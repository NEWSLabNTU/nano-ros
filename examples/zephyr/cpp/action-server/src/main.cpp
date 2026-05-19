/**
 * @file main.cpp
 * @brief Zephyr C++ action server (Phase 168.4 collapsed shape).
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

LOG_MODULE_REGISTER(nros_cpp_action_server, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

static nros_logger_t g_logger = nullptr;
static nros::ActionServer<Fibonacci>* g_srv = nullptr;
static int g_goal_count = 0;

static nros::GoalResponse on_goal(const uint8_t uuid[16], const Fibonacci::Goal& goal)
{
    if (goal.order < 0 || goal.order >= 64) return nros::GoalResponse::Reject;

    g_goal_count++;
    NROS_LOG_INFO(g_logger, "Goal received: order=%d", goal.order);

    int32_t a = 0, b = 1;
    Fibonacci::Result result;
    for (int32_t i = 0; i < goal.order && i < 64; i++) {
        result.sequence.push_back(a);
        if (i > 0 && (i % 3 == 0 || i == goal.order - 1)) {
            Fibonacci::Feedback fb;
            for (uint32_t k = 0; k < result.sequence.length(); k++)
                fb.sequence.push_back(result.sequence[k]);
            g_srv->publish_feedback(uuid, fb);
        }
        int32_t next = a + b; a = b; b = next;
    }

    if (g_srv->complete_goal(uuid, result).ok()) {
        NROS_LOG_INFO(g_logger, "Goal completed (len=%d)", result.sequence.length());
    }
    return nros::GoalResponse::AcceptAndExecute;
}

int nros_app_main(int argc, char **argv)
{
    (void)argc; (void)argv;
    LOG_INF("nros Zephyr C++ Action Server");

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
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_action_server"), 1);
    g_logger = node.get_logger();

    nros::ActionServer<Fibonacci> srv;
    NROS_TRY_RET(node.create_action_server(srv, "/fibonacci"), 1);
    g_srv = &srv;
    srv.set_goal_callback(on_goal);

    LOG_INF("Waiting for goal requests...");
    while (true) nros::spin_once(100);

    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
