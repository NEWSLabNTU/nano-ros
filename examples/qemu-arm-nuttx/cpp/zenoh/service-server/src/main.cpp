/// @file main.cpp
/// @brief C++ service server — AddTwoInts (NuttX QEMU)

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
    printf("nros C++ Service Server (NuttX)\n");
    // Wait for NuttX networking to come up (mirrors the C examples).
    sleep(5);
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_service_server");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    ret = node.create_service(srv, "/add_two_ints");
    if (!ret.ok()) { printf("create_service failed\n"); nros::shutdown(); return; }

    printf("Service server ready\n");
    printf("Waiting for requests\n");
    int req_count = 0;
    for (int poll = 0; poll < 50000; poll++) {
        nros::spin_once(10);
        example_interfaces::srv::AddTwoInts::Request req;
        int64_t seq_id = 0;
        while (srv.try_recv_request(req, seq_id)) {
            req_count++;
            example_interfaces::srv::AddTwoInts::Response resp;
            resp.sum = req.a + req.b;
            printf("Request [%d]: %d + %d = %d\n", req_count, (int)req.a, (int)req.b, (int)resp.sum);
            srv.send_reply(seq_id, resp);
        }
    }
    printf("Service server done (%d requests)\n", req_count);
    nros::shutdown();
}
