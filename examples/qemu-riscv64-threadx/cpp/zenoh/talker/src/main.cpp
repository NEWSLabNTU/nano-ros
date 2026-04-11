/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (ThreadX RISC-V QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Talker (ThreadX RISC-V QEMU)\n");

    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_talker");
    if (!ret.ok()) { printf("create_node failed: %d\n", ret.raw()); nros::shutdown(); return; }
    printf("Node created\n");

    nros::Publisher<std_msgs::msg::Int32> pub;
    ret = node.create_publisher(pub, "/chatter");
    if (!ret.ok()) { printf("create_publisher failed: %d\n", ret.raw()); nros::shutdown(); return; }

    printf("Publishing messages...\n");
    int count = 0;
    for (;;) {
        for (int s = 0; s < 100; s++) nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        msg.data = count;
        ret = pub.publish(msg);
        if (ret.ok()) printf("Published: %d\n", count);
        else printf("Publish failed: %d\n", ret.raw());
        count++;
    }
}
