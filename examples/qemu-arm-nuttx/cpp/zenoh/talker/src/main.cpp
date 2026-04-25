/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (NuttX QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

extern "C" int sleep(unsigned int);
extern "C" void app_main(void) {
    printf("nros C++ Talker (NuttX)\n");

    // Wait for NuttX networking to come up (mirrors the C examples).
    sleep(5);
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
