/**
 * @file main.c
 * @brief Zephyr C service server example using nros-c API (XRCE)
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>
#include <nros/executor.h>
#include <xrce_zephyr.h>

#include "example_interfaces.h"

LOG_MODULE_REGISTER(nros_xrce_service_server, LOG_LEVEL_INF);

static int g_request_count = 0;

static bool service_callback(const uint8_t* request_data,
                             size_t request_len,
                             uint8_t* response_data,
                             size_t response_capacity,
                             size_t* response_len,
                             void* context) {
    (void)context;

    example_interfaces_srv_add_two_ints_request request;
    if (example_interfaces_srv_add_two_ints_request_deserialize(
            &request, request_data, request_len) != 0) {
        LOG_ERR("Failed to deserialize request");
        return false;
    }

    g_request_count++;

    example_interfaces_srv_add_two_ints_response response;
    example_interfaces_srv_add_two_ints_response_init(&response);
    response.sum = request.a + request.b;

    LOG_INF("Request [%d]: %lld + %lld = %lld",
            g_request_count,
            (long long)request.a,
            (long long)request.b,
            (long long)response.sum);

    int32_t len = example_interfaces_srv_add_two_ints_response_serialize(
        &response, response_data, response_capacity);
    if (len < 0) {
        LOG_ERR("Failed to serialize response");
        return false;
    }

    *response_len = (size_t)len;
    return true;
}

int main(void)
{
    LOG_INF("nros Zephyr Service Server (XRCE)");

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
    ret = nros_node_init(&node, &support, "zephyr_xrce_service_server", "/");
    if (ret != NROS_RET_OK) {
        LOG_ERR("Node init failed: %d", ret);
        return 1;
    }

    nros_message_type_t type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
        .serialized_size_max = 256,
    };

    nros_service_t service = nros_service_get_zero_initialized();
    ret = nros_service_init(&service, &node, &type, "/add_two_ints", service_callback, NULL);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Service init failed: %d", ret);
        return 1;
    }

    nros_executor_t executor = nros_executor_get_zero_initialized();
    ret = nros_executor_init(&executor, &support, 4);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Executor init failed: %d", ret);
        return 1;
    }

    ret = nros_executor_add_service(&executor, &service);
    if (ret != NROS_RET_OK) {
        LOG_ERR("Add service failed: %d", ret);
        return 1;
    }

    LOG_INF("Waiting for requests...");

    nros_executor_spin_period(&executor, 100000000ULL);

    nros_executor_fini(&executor);
    nros_service_fini(&service);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}
