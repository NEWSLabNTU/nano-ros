/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (FreeRTOS QEMU)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>
#include <nros/app_config.h>
#include "std_msgs.hpp"

// Phase 88.16.H — set after `nros::create_node`; used by post-init
// diagnostics. nullptr before init = `NROS_LOG_*` silently drops.
static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Talker (FreeRTOS)\n");

    NROS_TRY_RET(nros::init(NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_talker"), 1);
    g_logger = node.get_logger();
    nros_log_init();
    printf("Node created\n");

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    printf("Publishing messages...\n");
    int count = 0;
    for (;;) {
        for (int s = 0; s < 100; s++) nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        msg.data = count;
        nros::Result ret = pub.publish(msg);
        if (ret.ok()) NROS_LOG_INFO(g_logger, "Published: %d", count);
        else NROS_LOG_INFO(g_logger, "Publish failed: %d", ret.raw());
        count++;
    }
}

NROS_APP_MAIN_REGISTER_VOID()
