/// @file main.cpp
/// @brief C++ listener — subscribes to std_msgs/Int32 on /chatter (NuttX QEMU)

#include <cstdint>
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
    printf("nros C++ Listener (NuttX)\n");

    // Re-seed /dev/urandom (see talker for rationale). Unique seed per example.
    if (FILE* urandom = fopen("/dev/urandom", "wb")) {
        const uint8_t seed[4] = {10, 0, 2, 41};
        fwrite(seed, 1, sizeof(seed), urandom);
        fclose(urandom);
    }

    // Wait for NuttX networking to come up (mirrors the C examples).
    sleep(5);
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_listener");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::Subscription<std_msgs::msg::Int32> sub;
    ret = node.create_subscription(sub, "/chatter");
    if (!ret.ok()) { printf("create_subscription failed\n"); nros::shutdown(); return; }

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
