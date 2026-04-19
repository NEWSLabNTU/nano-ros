/**
 * @file main.c
 * @brief Zephyr C service client example using nros-c API (XRCE)
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/client.h>
#include <nros/executor.h>
#include <xrce_zephyr.h>

#include "example_interfaces.h"

LOG_MODULE_REGISTER(nros_xrce_service_client, LOG_LEVEL_INF);

int main(void)
{
    LOG_INF("nros Zephyr Service Client (XRCE)");

    if (xrce_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    if (xrce_zephyr_init(CONFIG_NROS_XRCE_AGENT_ADDR,
                         CONFIG_NROS_XRCE_AGENT_PORT) != 0) {
        LOG_ERR("XRCE transport init failed");
        return 1;
    }

    nros_support_t support = nros_support_get_zero_initialized();
    nros_ret_t ret = nros_support_init(
        &support,
        CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
        CONFIG_NROS_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Support init failed: %d", ret);
        return 1;
    }

    nros_node_t node = nros_node_get_zero_initialized();
    ret = nros_node_init(&node, &support, "zephyr_xrce_service_client", "/");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Node init failed: %d", ret);
        return 1;
    }

    nros_service_type_t type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    nros_client_t client = nros_client_get_zero_initialized();
    ret = nros_client_init(&client, &node, &type, "/add_two_ints");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Client init failed: %d", ret);
        return 1;
    }

    nros_executor_t executor;
    ret = nros_executor_init(&executor, &support, 4);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Executor init failed: %d", ret);
        nros_client_fini(&client);
        nros_node_fini(&node);
        nros_support_fini(&support);
        return 1;
    }

    ret = nros_executor_add_client(&executor, &client);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Failed to register client with executor: %d", ret);
        nros_executor_fini(&executor);
        nros_client_fini(&client);
        nros_node_fini(&node);
        nros_support_fini(&support);
        return 1;
    }

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
