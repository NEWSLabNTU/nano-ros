/// @file main.cpp
/// @brief C++ listener — subscribes to std_msgs/Int32 on /chatter (ThreadX RISC-V QEMU)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Listener (ThreadX RISC-V QEMU)\n");
    NROS_TRY_RET(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_listener"), 1);
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
            printf("Received: %d\n", msg.data);
        }
    }
}

NROS_APP_MAIN_REGISTER_VOID()
