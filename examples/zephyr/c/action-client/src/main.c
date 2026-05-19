/**
 * @file main.c
 * @brief Zephyr C action client (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

#if defined(CONFIG_NROS_RMW_ZENOH)
#include <zpico_zephyr.h>
#elif defined(CONFIG_NROS_RMW_XRCE)
#include <xrce_zephyr.h>
#endif

LOG_MODULE_REGISTER(nros_action_client, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/action.h>
#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/log.h>
#include <nros/node.h>

#include "example_interfaces.h"

static nros_logger_t g_logger = NULL;

int nros_app_main(int argc, char **argv)
{
    (void)argc; (void)argv;
    LOG_INF("nros Zephyr Action Client");

#if defined(CONFIG_NROS_RMW_ZENOH)
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { LOG_ERR("Network not ready"); return 1; }
#elif defined(CONFIG_NROS_RMW_XRCE)
    if (xrce_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { return 1; }
#endif

    nros_support_t support = nros_support_get_zero_initialized();
#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_CHECK_RET(nros_support_init(&support, CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    NROS_CHECK_RET(nros_support_init_named(&support,
        CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
        CONFIG_NROS_DOMAIN_ID, "xrce_action_client"), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE}=y"
#endif

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_action_client", "/"), 1);
    g_logger = nros_node_get_logger(&node);

    nros_action_type_t fib_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_action_client_t client = nros_action_client_get_zero_initialized();
    NROS_CHECK_RET(nros_action_client_init(&client, &node, "/fibonacci", &fib_type), 1);

    nros_executor_t executor = nros_executor_get_zero_initialized();
    NROS_CHECK_RET(nros_executor_init(&executor, &support, 4), 1);
    NROS_CHECK_RET(nros_executor_register_action_client(&executor, &client), 1);

    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;

    uint8_t goal_buf[64];
    int32_t goal_len = example_interfaces_action_fibonacci_goal_serialize(&goal, goal_buf, sizeof(goal_buf));
    if (goal_len < 0) goto cleanup;

    LOG_INF("Sending goal: order=%d", goal.order);
    nros_goal_uuid_t goal_uuid;
    nros_ret_t ret = nros_action_send_goal(&client, &executor, goal_buf, (size_t)goal_len, &goal_uuid);
    if (ret != NROS_RET_OK) { LOG_ERR("Send goal failed: %d", ret); goto cleanup; }

    nros_goal_status_t final_status;
    uint8_t result_buf[512];
    size_t result_len = 0;
    ret = nros_action_get_result(&client, &executor, &goal_uuid, &final_status,
                                  result_buf, sizeof(result_buf), &result_len);
    if (ret == NROS_RET_OK) {
        LOG_INF("Result status: %s", nros_goal_status_to_string(final_status));
        example_interfaces_action_fibonacci_result result;
        if (example_interfaces_action_fibonacci_result_deserialize(&result, result_buf, result_len) == 0) {
            LOG_INF("Sequence length: %u", result.sequence.size);
        }
    } else {
        LOG_ERR("Get result failed: %d", ret);
    }

cleanup:
    nros_executor_fini(&executor);
    nros_action_client_fini(&client);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
