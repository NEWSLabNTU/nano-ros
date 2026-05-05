/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (ThreadX Linux)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Talker (ThreadX Linux)\n");

    NROS_TRY_RET(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_talker"), 1);
    printf("Node created\n");

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

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

NROS_APP_MAIN_REGISTER_VOID()
