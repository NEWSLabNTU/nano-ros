/**
 * @file main.cpp
 * @brief Zephyr C++ action server example using nros-cpp API
 *
 * This example demonstrates a Fibonacci action server on Zephyr RTOS
 * using the nros C++ API (nros::init, nros::Node, nros::ActionServer<A>).
 * Uses manual-poll with spin_once() + try_recv_goal().
 * The nros module handles zenoh initialization and platform support.
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

/* ============================================================================
 * Application
 * ============================================================================ */

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

    /* Create action server (manual-poll) */
    nros::ActionServer<example_interfaces::action::Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) {
        LOG_ERR("Action server creation failed: %d", ret.raw());
        nros::shutdown();
        return 1;
    }

    /* Spin + poll loop */
    LOG_INF("Waiting for goal requests...");

    int goal_count = 0;

    while (true) {
        nros::spin_once(100);

        example_interfaces::action::Fibonacci::Goal goal;
        uint8_t goal_id[16];
        while (srv.try_recv_goal(goal, goal_id)) {
            goal_count++;
            LOG_INF("Goal received: order=%d", goal.order);

            /* Compute Fibonacci sequence with feedback */
            int32_t a = 0;
            int32_t b = 1;

            example_interfaces::action::Fibonacci::Result result;

            for (int32_t i = 0; i < goal.order && i < 64; i++) {
                result.sequence.push_back(a);

                /* Publish feedback periodically */
                if (i > 0 && (i % 3 == 0 || i == goal.order - 1)) {
                    example_interfaces::action::Fibonacci::Feedback fb;
                    for (uint32_t k = 0; k < result.sequence.length(); k++) {
                        fb.sequence.push_back(result.sequence[k]);
                    }
                    srv.publish_feedback(goal_id, fb);
                }

                int32_t next = a + b;
                a = b;
                b = next;
            }

            /* Complete goal */
            ret = srv.complete_goal(goal_id, result);
            if (ret.ok()) {
                LOG_INF("Goal completed (sequence length=%d)", result.sequence.length());
            } else {
                LOG_ERR("Failed to complete goal: %d", ret.raw());
            }
        }
    }

    /* Cleanup (unreachable in this example) */
    nros::shutdown();

    return 0;
}
