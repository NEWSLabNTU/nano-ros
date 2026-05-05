/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (ThreadX Linux)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>
#include "std_msgs.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Talker (ThreadX Linux)\n");

    NROS_CHECK(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID));

    nros::Node node;
    NROS_CHECK(nros::create_node(node, "cpp_talker"));
    printf("Node created\n");

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_CHECK(node.create_publisher(pub, "/chatter"));

    printf("Publishing messages...\n");
    for (int i = 0; i < 10; i++) {
        for (int s = 0; s < 100; s++) nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        msg.data = i;
        nros::Result ret = pub.publish(msg);
        if (ret.ok()) printf("Published: %d\n", i);
        else printf("Publish failed: %d\n", ret.raw());
    }
    printf("Done publishing 10 messages.\n");
    nros::shutdown();
}
