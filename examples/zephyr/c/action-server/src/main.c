/**
 * @file main.c
 * @brief Zephyr C action server (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>
#include <string.h>

#if defined(CONFIG_NROS_RMW_ZENOH)
#include <zpico_zephyr.h>
#elif defined(CONFIG_NROS_RMW_XRCE)
#include <xrce_zephyr.h>
#endif

LOG_MODULE_REGISTER(nros_action_server, LOG_LEVEL_INF);

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
static int g_goal_count = 0;

static nros_goal_response_t goal_callback(
    nros_action_server_t* server, const nros_goal_handle_t* goal,
    const uint8_t* goal_request, size_t goal_len, void* context)
{
    (void)server; (void)context;
    example_interfaces_action_fibonacci_goal goal_msg;
    if (example_interfaces_action_fibonacci_goal_deserialize(&goal_msg, goal_request, goal_len) != 0) {
        return NROS_GOAL_REJECT;
    }
    NROS_LOG_INFO(g_logger, "Goal request: order=%d", goal_msg.order);
    if (goal_msg.order < 0 || goal_msg.order >= 64) return NROS_GOAL_REJECT;
    return NROS_GOAL_ACCEPT_AND_EXECUTE;
}

static nros_cancel_response_t cancel_callback(
    nros_action_server_t* server, const nros_goal_handle_t* goal, void* context)
{
    (void)server; (void)goal; (void)context;
    return NROS_CANCEL_ACCEPT;
}

static void accepted_callback(nros_action_server_t* server, const nros_goal_handle_t* goal, void* context)
{
    (void)context;
    g_goal_count++;
    int32_t order = 10;
    nros_ret_t ret = nros_action_execute(server, goal);
    if (ret != NROS_RET_OK) return;

    example_interfaces_action_fibonacci_feedback fb;
    example_interfaces_action_fibonacci_feedback_init(&fb);

    for (int32_t i = 0; i <= order; i++) {
        int32_t val;
        if (i == 0) val = 0;
        else if (i == 1) val = 1;
        else val = fb.sequence.data[i - 1] + fb.sequence.data[i - 2];
        fb.sequence.data[i] = val;
        fb.sequence.size = (uint32_t)(i + 1);

        uint8_t fb_buf[512];
        int32_t fb_len = example_interfaces_action_fibonacci_feedback_serialize(&fb, fb_buf, sizeof(fb_buf));
        if (fb_len > 0) nros_action_publish_feedback(server, goal, fb_buf, (size_t)fb_len);
    }

    example_interfaces_action_fibonacci_result result;
    example_interfaces_action_fibonacci_result_init(&result);
    result.sequence.size = fb.sequence.size;
    memcpy(result.sequence.data, fb.sequence.data, fb.sequence.size * sizeof(int32_t));

    uint8_t result_buf[512];
    int32_t result_len = example_interfaces_action_fibonacci_result_serialize(&result, result_buf, sizeof(result_buf));
    if (result_len > 0) nros_action_succeed(server, goal, result_buf, (size_t)result_len);
}

int nros_app_main(int argc, char **argv)
{
    (void)argc; (void)argv;
    LOG_INF("nros Zephyr Action Server");

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
        CONFIG_NROS_DOMAIN_ID, "xrce_action_server"), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE}=y"
#endif

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_action_server", "/"), 1);
    g_logger = nros_node_get_logger(&node);

    nros_action_type_t fib_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    nros_action_server_t server = nros_action_server_get_zero_initialized();
    NROS_CHECK_RET(nros_action_server_init(&server, &node, "/fibonacci", &fib_type,
        goal_callback, cancel_callback, accepted_callback, NULL), 1);

    nros_executor_t executor = nros_executor_get_zero_initialized();
    NROS_CHECK_RET(nros_executor_init(&executor, &support, 8), 1);
    NROS_CHECK_RET(nros_executor_register_action_server(&executor, &server), 1);

    LOG_INF("Waiting for goals...");
    nros_executor_spin_period(&executor, 100000000ULL);

    nros_executor_fini(&executor);
    nros_action_server_fini(&server);
    nros_node_fini(&node);
    nros_support_fini(&support);
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
