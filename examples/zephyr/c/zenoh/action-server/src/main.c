/**
 * @file main.c
 * @brief Zephyr C action server example using nros-c API (Zenoh)
 *
 * Demonstrates a Fibonacci action server on Zephyr RTOS using the
 * nros C API with Zenoh transport. Accepts goals, computes the
 * Fibonacci sequence with feedback, and returns the result.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/action.h>
#include <nros/executor.h>
#include <zpico_zephyr.h>

#include "example_interfaces.h"

LOG_MODULE_REGISTER(nros_action_server, LOG_LEVEL_INF);

static int g_goal_count = 0;

static nros_goal_response_t goal_callback(
    nros_action_server_t* server,
    const nros_goal_handle_t* goal,
    const uint8_t* goal_request,
    size_t goal_len,
    void* context)
{
    (void)server;
    (void)context;

    example_interfaces_action_fibonacci_goal goal_msg;
    if (example_interfaces_action_fibonacci_goal_deserialize(
            &goal_msg, goal_request, goal_len) != 0) {
        LOG_ERR("Failed to deserialize goal");
        return NROS_GOAL_REJECT;
    }

    LOG_INF("Goal request: order=%d (uuid=%02x%02x...)",
            goal_msg.order, goal->uuid.uuid[0], goal->uuid.uuid[1]);

    if (goal_msg.order < 0 || goal_msg.order >= 64) {
        LOG_INF("  -> REJECTED (order out of range)");
        return NROS_GOAL_REJECT;
    }

    LOG_INF("  -> ACCEPTED");
    return NROS_GOAL_ACCEPT_AND_EXECUTE;
}

static nros_cancel_response_t cancel_callback(
    nros_action_server_t* server,
    const nros_goal_handle_t* goal,
    void* context)
{
    (void)server;
    (void)context;
    LOG_INF("Cancel request (uuid=%02x%02x...)",
            goal->uuid.uuid[0], goal->uuid.uuid[1]);
    return NROS_CANCEL_ACCEPT;
}

static void accepted_callback(nros_action_server_t* server, const nros_goal_handle_t* goal, void* context)
{
    (void)server;
    (void)context;
    g_goal_count++;

    LOG_INF("Executing goal [%d] (uuid=%02x%02x...)",
            g_goal_count,
            goal->uuid.uuid[0], goal->uuid.uuid[1]);

    /* NOTE: In a real application, you would store the parsed goal data
     * during goal_callback (e.g., in a struct pointed to by goal->context).
     * For this example, we use a fixed order of 10. */
    int32_t order = 10;

    nros_ret_t ret = nros_action_execute(server, goal);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Failed to set executing state: %d", ret);
        return;
    }

    /* Compute Fibonacci sequence with feedback */
    example_interfaces_action_fibonacci_feedback fb;
    example_interfaces_action_fibonacci_feedback_init(&fb);

    for (int32_t i = 0; i <= order; i++) {
        int32_t val;
        if (i == 0) {
            val = 0;
        } else if (i == 1) {
            val = 1;
        } else {
            val = fb.sequence.data[i - 1] + fb.sequence.data[i - 2];
        }
        fb.sequence.data[i] = val;
        fb.sequence.size = (uint32_t)(i + 1);

        uint8_t fb_buf[512];
        int32_t fb_len = example_interfaces_action_fibonacci_feedback_serialize(
            &fb, fb_buf, sizeof(fb_buf));
        if (fb_len > 0) {
            ret = nros_action_publish_feedback(server, goal, fb_buf, (size_t)fb_len);
            if (ret != NROS_RET_OK) {
                LOG_ERR("Failed to publish feedback: %d", ret);
            }
        }
    }

    /* Send result */
    example_interfaces_action_fibonacci_result result;
    example_interfaces_action_fibonacci_result_init(&result);
    result.sequence.size = fb.sequence.size;
    memcpy(result.sequence.data, fb.sequence.data,
           fb.sequence.size * sizeof(int32_t));

    uint8_t result_buf[512];
    int32_t result_len = example_interfaces_action_fibonacci_result_serialize(
        &result, result_buf, sizeof(result_buf));
    if (result_len > 0) {
        ret = nros_action_succeed(server, goal, result_buf, (size_t)result_len);
        if (ret != NROS_RET_OK) {
            LOG_ERR("Failed to send result: %d", ret);
        } else {
            LOG_INF("  Goal SUCCEEDED");
        }
    }
}

int main(void)
{
    LOG_INF("nros Zephyr Action Server (Zenoh)");

    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    nros_support_t support = nros_support_get_zero_initialized();
    nros_ret_t ret = nros_support_init(&support, CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Support init failed: %d", ret);
        return 1;
    }

    nros_node_t node = nros_node_get_zero_initialized();
    ret = nros_node_init(&node, &support, "zephyr_action_server", "/");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Node init failed: %d", ret);
        return 1;
    }

    nros_action_type_t fib_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_action_server_t server = nros_action_server_get_zero_initialized();
    ret = nros_action_server_init(&server, &node, "/fibonacci", &fib_type,
                                  goal_callback, cancel_callback, accepted_callback, NULL);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Action server init failed: %d", ret);
        return 1;
    }

    nros_executor_t executor = nros_executor_get_zero_initialized();
    ret = nros_executor_init(&executor, &support, 8);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Executor init failed: %d", ret);
        return 1;
    }

    ret = nros_executor_add_action_server(&executor, &server);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Add action server failed: %d", ret);
        return 1;
    }

    LOG_INF("Waiting for goals...");

    nros_executor_spin_period(&executor, 100000000ULL);

    nros_executor_fini(&executor);
    nros_action_server_fini(&server);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}
