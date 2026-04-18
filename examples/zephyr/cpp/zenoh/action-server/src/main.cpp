/**
 * @file main.cpp
 * @brief Zephyr C++ action server example using nros-cpp API
 *
 * Callback-based action server: set_goal_callback registers a stateless
 * goal handler that computes Fibonacci inline, publishing feedback and
 * completing the goal before returning.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

extern "C" {
#include <zpico_zephyr.h>
}

#include <nros/nros.hpp>

// Generated C++ action bindings
#include "example_interfaces.hpp"

LOG_MODULE_REGISTER(nros_cpp_action_server, LOG_LEVEL_INF);

using Fibonacci = example_interfaces::action::Fibonacci;

static nros::ActionServer<Fibonacci>* g_srv = nullptr;
static int g_goal_count = 0;

static nros::GoalResponse on_goal(const uint8_t uuid[16], const Fibonacci::Goal& goal) {
    if (goal.order < 0 || goal.order >= 64) {
        LOG_WRN("Goal rejected (order out of range): %d", goal.order);
        return nros::GoalResponse::Reject;
    }

    g_goal_count++;
    LOG_INF("Goal received: order=%d", goal.order);

    int32_t a = 0;
    int32_t b = 1;
    Fibonacci::Result result;

    for (int32_t i = 0; i < goal.order && i < 64; i++) {
        result.sequence.push_back(a);

        if (i > 0 && (i % 3 == 0 || i == goal.order - 1)) {
            Fibonacci::Feedback fb;
            for (uint32_t k = 0; k < result.sequence.length(); k++) {
                fb.sequence.push_back(result.sequence[k]);
            }
            g_srv->publish_feedback(uuid, fb);
        }

        int32_t next = a + b;
        a = b;
        b = next;
    }

    if (g_srv->complete_goal(uuid, result).ok()) {
        LOG_INF("Goal completed (sequence length=%d)", result.sequence.length());
    } else {
        LOG_ERR("Failed to complete goal");
    }
    return nros::GoalResponse::AcceptAndExecute;
}

int main(void)
{
    LOG_INF("nros Zephyr C++ Action Server");
    LOG_INF("===============================");

    /* Wait for network interface */
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    /* Initialize nros session */
    nros::Result ret = nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID);
    if (!ret.ok()) {
        LOG_ERR("Init failed: %d", ret.raw());
        return 1;
    }

    /* Create node */
    nros::Node node;
    ret = nros::create_node(node, "zephyr_cpp_action_server");
    if (!ret.ok()) {
        LOG_ERR("Node creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Create action server (callback-based) */
    nros::ActionServer<Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) {
        LOG_ERR("Action server creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    g_srv = &srv;
    srv.set_goal_callback(on_goal);

    LOG_INF("Waiting for goal requests...");

    while (true) {
        nros::spin_once(100);
    }

    /* Unreachable */
    nros::shutdown();
    return 0;
}
