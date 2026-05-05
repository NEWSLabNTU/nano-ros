/// @file main.cpp
/// @brief C++ talker — publishes std_msgs/Int32 on /chatter (NuttX QEMU)

#include <cstdint>
#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include "std_msgs.hpp"

#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

extern "C" int sleep(unsigned int);
int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Talker (NuttX)\n");

    // Re-seed /dev/urandom — without this, two NuttX QEMU instances generate
    // identical zenoh session IDs (NuttX's xorshift128 PRNG starts with a
    // fixed seed) and zenohd rejects the second connection with MAX_LINKS.
    // Each C++ example uses a unique 4-byte seed; values here ({10,0,2,40+})
    // are kept disjoint from the C examples ({10,0,2,30+}).
    if (FILE* urandom = fopen("/dev/urandom", "wb")) {
        const uint8_t seed[4] = {10, 0, 2, 40};
        fwrite(seed, 1, sizeof(seed), urandom);
        fclose(urandom);
    }

    // Wait for NuttX networking to come up (mirrors the C examples).
    sleep(5);
    NROS_TRY_RET(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_talker"), 1);
    printf("Node created\n");

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);
    nros::Result ret;

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

NROS_APP_MAIN_REGISTER_VOID()
