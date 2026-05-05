/**
 * @file main.c
 * @brief Zephyr C action client example using nros-c API (XRCE)
 *
 * Demonstrates a Fibonacci action client on Zephyr RTOS using the
 * nros C API with XRCE-DDS transport. Sends a goal, waits for the
 * result, and prints the final Fibonacci sequence.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_xrce_action_client, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/action.h>
#include <nros/check.h>
#include <nros/init.h>
#include <nros/node.h>

#include "example_interfaces.h"

int main(void)
{
    LOG_INF("nros Zephyr Action Client (XRCE)");

    /* Initialize support context (handles network wait + transport setup) */
    nros_support_t support = nros_support_get_zero_initialized();
    NROS_CHECK_RET(nros_support_init_named(
        &support,
        CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
        CONFIG_NROS_DOMAIN_ID,
        "xrce_action_client"), 1);

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_xrce_action_client", "/"), 1);

    nros_action_type_t fib_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_action_client_t client = nros_action_client_get_zero_initialized();
    NROS_CHECK_RET(nros_action_client_init(&client, &node, "/fibonacci", &fib_type), 1);
    nros_ret_t ret = NROS_RET_OK;

    /* Send goal: compute Fibonacci sequence of order 10 */
    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;

    uint8_t goal_buf[64];
    int32_t goal_len = example_interfaces_action_fibonacci_goal_serialize(
        &goal, goal_buf, sizeof(goal_buf));
    if (goal_len < 0) {
        LOG_ERR("Serialize failed");
        goto cleanup;
    }

    LOG_INF("Sending goal: order=%d", goal.order);

    nros_goal_uuid_t goal_uuid;
    ret = nros_action_send_goal(&client, goal_buf, (size_t)goal_len, &goal_uuid);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Send goal failed: %d", ret);
        goto cleanup;
    }

    LOG_INF("Goal sent (uuid=%02x%02x%02x%02x...), waiting for result...",
            goal_uuid.uuid[0], goal_uuid.uuid[1],
            goal_uuid.uuid[2], goal_uuid.uuid[3]);

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(&client, &goal_uuid, &final_status,
                                 result_buf, sizeof(result_buf), &result_len);

    if (ret == NROS_RET_OK) {
        LOG_INF("Result status: %s", nros_goal_status_to_string(final_status));

        example_interfaces_action_fibonacci_result result;
        if (example_interfaces_action_fibonacci_result_deserialize(
                &result, result_buf, result_len) == 0) {
            LOG_INF("Sequence length: %u", result.sequence.size);
        } else {
            LOG_ERR("Deserialize result failed");
        }
    } else if (ret == NROS_RET_TIMEOUT) {
        LOG_ERR("Timeout waiting for result");
    } else {
        LOG_ERR("Get result failed: %d", ret);
    }

cleanup:
    nros_action_client_fini(&client);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}
