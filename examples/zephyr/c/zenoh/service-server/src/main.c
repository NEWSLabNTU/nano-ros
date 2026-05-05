/**
 * @file main.c
 * @brief Zephyr C service server example using nros-c API (Zenoh)
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>

LOG_MODULE_REGISTER(nros_service_server, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>
#include <zpico_zephyr.h>

#include "example_interfaces.h"

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
    LOG_INF("nros Zephyr Service Server (Zenoh)");

    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    nros_support_t support = nros_support_get_zero_initialized();
    NROS_CHECK_RET(nros_support_init(&support, CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_service_server", "/"), 1);

    nros_service_type_t type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    nros_service_t service = nros_service_get_zero_initialized();
    NROS_CHECK_RET(nros_service_init(&service, &node, &type, "/add_two_ints", service_callback, NULL), 1);

    nros_executor_t executor = nros_executor_get_zero_initialized();
    NROS_CHECK_RET(nros_executor_init(&executor, &support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_service(&executor, &service), 1);

    LOG_INF("Waiting for requests...");

    nros_executor_spin_period(&executor, 100000000ULL);

    nros_executor_fini(&executor);
    nros_service_fini(&service);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}
