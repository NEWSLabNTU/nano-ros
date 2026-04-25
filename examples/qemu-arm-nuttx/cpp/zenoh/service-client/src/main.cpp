/// @file main.cpp
/// @brief C++ service client — AddTwoInts (NuttX QEMU, async Future)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

extern "C" int sleep(unsigned int);
extern "C" void app_main(void) {
    printf("nros C++ Service Client (NuttX)\n");
    // Wait for NuttX networking to come up (mirrors the C examples).
    sleep(5);
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_service_client");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::Client<example_interfaces::srv::AddTwoInts> client;
    ret = node.create_client(client, "/add_two_ints");
    if (!ret.ok()) { printf("create_client failed\n"); nros::shutdown(); return; }

    printf("Service client ready\n");
    struct { int64_t a, b; } cases[] = {{5,3},{10,20},{100,200},{-5,10}};
    int ok_count = 0;
    for (int i = 0; i < 4; i++) {
        example_interfaces::srv::AddTwoInts::Request req;
        req.a = cases[i].a; req.b = cases[i].b;
        example_interfaces::srv::AddTwoInts::Response resp;
        auto fut = client.send_request(req);
        if (fut.is_consumed()) { printf("Call [%d] send failed\n", i+1); continue; }
        ret = fut.wait(nros::global_handle(), 5000, resp);
        if (ret.ok()) {
            printf("Response: %d + %d = %d", (int)req.a, (int)req.b, (int)resp.sum);
            if (resp.sum == req.a + req.b) { printf(" [OK]\n"); ok_count++; }
            else printf(" [MISMATCH]\n");
        } else {
            printf("Call [%d] failed: %d\n", i+1, ret.raw());
        }
    }
    printf("All service calls completed (%d/%d succeeded)\n", ok_count, 4);
    nros::shutdown();
}
