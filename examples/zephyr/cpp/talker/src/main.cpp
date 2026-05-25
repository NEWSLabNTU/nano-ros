/**
 * @file main.cpp
 * @brief Zephyr C++ talker (Phase 168.4 collapsed shape).
 */

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/autoconf.h>

extern "C" {
#if defined(CONFIG_NROS_RMW_ZENOH)
#include <zpico_zephyr.h>
#elif defined(CONFIG_NROS_RMW_XRCE)
#include <xrce_zephyr.h>
#endif
}

LOG_MODULE_REGISTER(nros_cpp_talker, LOG_LEVEL_INF);

#define NROS_TRY_LOG(file, line, expr, ret) \
    LOG_ERR("%s:%d %s -> %d", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char **argv)
{
    (void)argc; (void)argv;
    LOG_INF("nros Zephyr C++ Talker");

#if defined(CONFIG_NROS_RMW_ZENOH)
    if (zpico_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { LOG_ERR("Network not ready"); return 1; }
#elif defined(CONFIG_NROS_RMW_XRCE)
    if (xrce_zephyr_wait_network(CONFIG_NROS_INIT_DELAY_MS) != 0) { return 1; }
#endif

#if defined(CONFIG_NROS_RMW_ZENOH)
    NROS_TRY_RET(nros::init(CONFIG_NROS_ZENOH_LOCATOR, CONFIG_NROS_DOMAIN_ID), 1);
#elif defined(CONFIG_NROS_RMW_XRCE)
    /* Distinct XRCE session name (Phase 177.9.F) — the client key is
     * `hash_session_key(session_name)`; the 2-arg `nros::init` defaults it to
     * "nros_cpp", which would collide with the listener's client key on the
     * same Agent. Use this process's node name. */
    NROS_TRY_RET(nros::init(CONFIG_NROS_XRCE_AGENT_ADDR ":" STRINGIFY(CONFIG_NROS_XRCE_AGENT_PORT),
                            CONFIG_NROS_DOMAIN_ID, "zephyr_cpp_talker"), 1);
#elif defined(CONFIG_NROS_RMW_CYCLONEDDS)
    NROS_TRY_RET(nros::init("", CONFIG_NROS_DOMAIN_ID), 1);
#else
#error "Phase 168.4 requires CONFIG_NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}=y"
#endif

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "zephyr_cpp_talker"), 1);
    g_logger = node.get_logger();

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    LOG_INF("Publishing messages...");
    int32_t count = 0;
    while (true) {
        count++;
        std_msgs::msg::Int32 msg;
        msg.data = count;
        nros::Result ret = pub.publish(msg);
        if (ret.ok()) NROS_LOG_INFO(g_logger, "Published: %d", count);
        else NROS_LOG_ERROR(g_logger, "Publish failed: %d", ret.raw());
        k_sleep(K_SECONDS(1));
    }
    nros::shutdown();
    return 0;
}

NROS_APP_MAIN_REGISTER_ZEPHYR()
