/// @file main.cpp
/// @brief C++ listener — subscribes to std_msgs/Int32 on /chatter (FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Listener (FreeRTOS)\n");
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_listener");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::Subscription<std_msgs::msg::Int32> sub;
    ret = node.create_subscription(sub, "/chatter");
    if (!ret.ok()) { printf("create_subscription failed\n"); nros::shutdown(); return; }

    printf("Waiting for messages...\n");
    int msg_count = 0;
    for (int poll = 0; poll < 100000 && msg_count < 10; poll++) {
        nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            msg_count++;
            printf("Received [%d]: %d\n", msg_count, msg.data);
        }
    }
    printf("Received %d messages\n", msg_count);
    nros::shutdown();
}
