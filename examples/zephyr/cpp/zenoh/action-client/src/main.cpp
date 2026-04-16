/**
 * @file main.cpp
 * @brief Zephyr C++ action client example using nros-cpp API
 *
 * This example demonstrates a Fibonacci action client on Zephyr RTOS
 * using the nros C++ API (nros::init, nros::Node, nros::ActionClient<A>).
 * Sends a goal using the Future-based API, polls for feedback, then gets
 * the result. The nros module handles zenoh initialization and platform support.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

extern "C" {
#include <zpico_zephyr.h>
}

#include <nros/nros.hpp>

// Generated C++ action bindings
#include "example_interfaces.hpp"

LOG_MODULE_REGISTER(nros_cpp_action_client, LOG_LEVEL_INF);

/* ============================================================================
 * Application
 * ============================================================================ */

int main(void)
{
    LOG_INF("nros Zephyr C++ Action Client");
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
    ret = nros::create_node(node, "zephyr_cpp_action_client");
    if (!ret.ok()) {
        LOG_ERR("Node creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Create action client */
    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    ret = node.create_action_client(client, "/fibonacci");
    if (!ret.ok()) {
        LOG_ERR("Action client creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Allow time for connection to stabilize */
    k_sleep(K_SECONDS(2));

    /* Send goal */
    int32_t order = 10;
    LOG_INF("Sending goal: order=%d", order);

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = order;

    auto goal_fut = client.send_goal_future(goal);
    if (goal_fut.is_consumed()) {
        LOG_ERR("Failed to send goal");
        nros::shutdown();
        return 1;
    }

    nros::ActionClient<example_interfaces::action::Fibonacci>::GoalAccept accept;
    ret = goal_fut.wait(nros::global_handle(), 10000, accept);
    if (!ret.ok() || !accept.accepted) {
        LOG_ERR("Goal not accepted: %d", ret.raw());
        nros::shutdown();
        return 1;
    }
    LOG_INF("Goal accepted: order=%d", order);

    /* Poll for feedback while waiting */
    for (int i = 0; i < 20; i++) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Feedback fb;
        while (client.try_recv_feedback(fb)) {
            LOG_INF("Feedback: sequence length=%d", fb.sequence.length());
        }
    }

    /* Get result (Future-based) */
    auto result_fut = client.get_result_future(accept.goal_id);
    if (result_fut.is_consumed()) {
        LOG_ERR("Failed to request result");
        nros::shutdown();
        return 1;
    }

    example_interfaces::action::Fibonacci::Result result;
    ret = result_fut.wait(nros::global_handle(), 30000, result);
    if (ret.ok()) {
        LOG_INF("Result received: sequence length=%d", result.sequence.length());
        LOG_INF("[OK]");
    } else {
        LOG_ERR("Failed to get result: %d", ret.raw());
        LOG_ERR("[FAIL]");
    }

    /* Cleanup */
    nros::shutdown();

    return 0;
}
