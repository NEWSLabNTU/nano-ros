/**
 * @file main.c
 * @brief Zephyr C talker example (Phase 168.4 collapsed shape).
 *
 * Single source, three RMW backends. RMW choice flows from
 * `prj-<rmw>.conf` overlay → Kconfig `CONFIG_NROS_RMW_<X>` →
 * `#if defined(CONFIG_NROS_RMW_<X>)` blocks below.
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

#include <nros/platform_zephyr.h>

LOG_MODULE_REGISTER(nros_talker, LOG_LEVEL_INF);

#define NROS_CHECK_LOG(file, line, expr, ret)                                                      \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/init.h>
#include <nros/log.h>
#include <nros/node.h>
#include <nros/publisher.h>

#include "std_msgs.h"

static nros_logger_t g_logger = NULL;

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    LOG_INF("nros Zephyr C Talker");

    if (nros_platform_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) {
        LOG_ERR("Network not ready");
        return 1;
    }

    nros_support_t support = nros_support_get_zero_initialized();
#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_CHECK_RET(nros_support_init(&support, CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID),
                   1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    NROS_CHECK_RET(nros_support_init_named(&support,
                                           CONFIG_NROS_XRCE_AGENT_ADDR
                                           ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                                           CONFIG_NROS_DOMAIN_ID, "xrce_talker"),
                   1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_CHECK_RET(nros_support_init(&support, "", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y via prj-<rmw>.conf overlay"
#endif

    nros_node_t node = nros_node_get_zero_initialized();
    NROS_CHECK_RET(nros_node_init(&node, &support, "zephyr_talker", "/"), 1);
    g_logger = nros_node_get_logger(&node);

    nros_publisher_t pub = nros_publisher_get_zero_initialized();
    NROS_CHECK_RET(
        nros_publisher_init(&pub, &node, std_msgs_msg_int32_get_type_support(), "/chatter"), 1);

    int32_t count = 0;
    std_msgs_msg_int32 msg;
    std_msgs_msg_int32_init(&msg);

    LOG_INF("Publishing messages...");

    while (1) {
        count++;
        msg.data = count;
        NROS_SOFTCHECK(std_msgs_msg_int32_publish(&pub, &msg));
        NROS_LOG_INFO(g_logger, "Published: %d", count);
        k_sleep(K_SECONDS(1));
    }

    nros_publisher_fini(&pub);
    nros_node_fini(&node);
    nros_support_fini(&support);

    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
