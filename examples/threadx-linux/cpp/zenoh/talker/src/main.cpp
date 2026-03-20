/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (ThreadX Linux)

#include <cstdio>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Talker (ThreadX Linux)\n");

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
    for (int i = 0; i < 10; i++) {
        for (int s = 0; s < 100; s++) nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        msg.data = i;
        ret = pub.publish(msg);
        if (ret.ok()) printf("Published: %d\n", i);
        else printf("Publish failed: %d\n", ret.raw());
    }
    printf("Done publishing 10 messages.\n");
    nros::shutdown();
}
