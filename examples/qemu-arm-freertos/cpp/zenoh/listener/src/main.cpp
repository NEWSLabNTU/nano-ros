/// @file main.cpp
/// @brief C++ listener — subscribes to std_msgs/Int32 on /chatter (FreeRTOS QEMU)

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

    printf("nros C++ Listener (FreeRTOS)\n");
    NROS_TRY_RET(nros::init(NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_listener"), 1);
    g_logger = node.get_logger();
    nros_log_init();
    printf("Node created\n");

    nros::Subscription<std_msgs::msg::Int32> sub;
    NROS_TRY_RET(node.create_subscription(sub, "/chatter"), 1);

    // Alternative: use Stream::wait_next for blocking reception
    // std_msgs::msg::Int32 msg;
    // sub.stream().wait_next(executor_handle, 1000, msg);

    printf("Waiting for messages...\n");
    for (;;) {
        nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            NROS_LOG_INFO(g_logger, "Received: %d", msg.data);
        }
    }
}

NROS_APP_MAIN_REGISTER_VOID()
