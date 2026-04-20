/**
 * @file main.cpp
 * @brief Zephyr C++ action client example using nros-cpp API
 *
 * Sends a goal asynchronously, polls for feedback, then retrieves
 * the result via Future::wait (executor-driven, no blocking zpico_get).
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
    using Fibonacci = example_interfaces::action::Fibonacci;
    nros::ActionClient<Fibonacci> client;
    ret = node.create_action_client(client, "/fibonacci");
    if (!ret.ok()) {
        LOG_ERR("Action client creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Warm-up spin — allow zenoh discovery to complete */
    for (int i = 0; i < 30; i++) {
        nros::spin_once(100);
    }

    /* Send goal (blocking convenience — spins executor internally) */
    int32_t order = 10;
    LOG_INF("Sending goal: order=%d", order);

    Fibonacci::Goal goal;
    goal.order = order;

    uint8_t goal_id[16];
    ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) {
        LOG_ERR("Failed to send goal: %d", ret.raw());
        nros::shutdown();
        return 1;
    }
    LOG_INF("Goal sent: order=%d", order);

    /* Poll for feedback while waiting for server to complete */
    for (int i = 0; i < 30; i++) {
        nros::spin_once(100);

        Fibonacci::Feedback fb;
        while (client.try_recv_feedback(fb)) {
            LOG_INF("Feedback: sequence length=%d", fb.sequence.length());
        }
    }

    /* Get result via async Future (executor-driven, no blocking zpico_get) */
    auto result_fut = client.get_result_future(goal_id);
    Fibonacci::Result result;
    ret = result_fut.wait(nros::global_handle(), 10000, result);
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
