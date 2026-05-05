/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (ThreadX RISC-V QEMU)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>
#include "std_msgs.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Talker (ThreadX RISC-V QEMU)\n");

    NROS_CHECK(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID));

    nros::Node node;
    NROS_CHECK(nros::create_node(node, "cpp_talker"));
    printf("Node created\n");

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_CHECK(node.create_publisher(pub, "/chatter"));

    printf("Publishing messages...\n");
    int count = 0;
    for (;;) {
        for (int s = 0; s < 100; s++) nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        msg.data = count;
        nros::Result ret = pub.publish(msg);
        if (ret.ok()) printf("Published: %d\n", count);
        else printf("Publish failed: %d\n", ret.raw());
        count++;
    }
}
