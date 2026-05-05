/**
 * @file main.c
 * @brief Zephyr C service client example using nros-c API (XRCE)
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_xrce_service_client, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/client.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>

#include "example_interfaces.h"

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    LOG_INF("nros Zephyr Service Client (XRCE)");

    /* Initialize support context (handles network wait + transport setup) */
    nros_support_t support = nros_support_get_zero_initialized();
    NROS_CHECK_RET(nros_support_init_named(
        &support,
        CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
        CONFIG_NROS_DOMAIN_ID,
        "xrce_service_client"), 1);

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_xrce_service_client", "/"), 1);

    nros_service_type_t type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    nros_client_t client = nros_client_get_zero_initialized();
    NROS_CHECK_RET(nros_client_init(&client, &node, &type, "/add_two_ints"), 1);

    nros_executor_t executor;
    NROS_CHECK_RET(nros_executor_init(&executor, &support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_client(&executor, &client), 1);
    nros_ret_t ret = NROS_RET_OK;

    LOG_INF("Calling service...");

    example_interfaces_srv_add_two_ints_request request;
    example_interfaces_srv_add_two_ints_request_init(&request);
    request.a = 5;
    request.b = 3;

    uint8_t req_buf[256];
    int32_t req_len = example_interfaces_srv_add_two_ints_request_serialize(
        &request, req_buf, sizeof(req_buf));
    if (req_len < 0) {
        LOG_ERR("Serialize failed");
        return 1;
    }

    uint8_t resp_buf[256];
    size_t resp_len = 0;
    ret = nros_client_call(&client, req_buf, (size_t)req_len,
                           resp_buf, sizeof(resp_buf), &resp_len);

    if (ret == NROS_RET_OK) {
        example_interfaces_srv_add_two_ints_response response;
        if (example_interfaces_srv_add_two_ints_response_deserialize(
                &response, resp_buf, resp_len) == 0) {
            LOG_INF("Result: %lld + %lld = %lld",
                    (long long)request.a,
                    (long long)request.b,
                    (long long)response.sum);
        } else {
            LOG_ERR("Deserialize failed");
        }
    } else if (ret == NROS_RET_TIMEOUT) {
        LOG_ERR("Timeout");
    } else {
        LOG_ERR("Call failed: %d", ret);
    }

    nros_executor_fini(&executor);
    nros_client_fini(&client);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
