/// @file main.cpp
/// @brief C++ service client — AddTwoInts (FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Service Client (FreeRTOS)\n");
    nros::Result ret = nros::init("tcp/192.0.3.1:7447", 0);
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
        ret = client.call(req, resp);
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
