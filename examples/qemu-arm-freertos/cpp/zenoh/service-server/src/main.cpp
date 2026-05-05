/// @file main.cpp
/// @brief C++ service server — AddTwoInts (FreeRTOS QEMU)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>
#include "example_interfaces.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Service Server (FreeRTOS)\n");
    NROS_CHECK(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID));

    nros::Node node;
    NROS_CHECK(nros::create_node(node, "cpp_service_server"));
    printf("Node created\n");

    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    NROS_CHECK(node.create_service(srv, "/add_two_ints"));

    printf("Service server ready\n");
    printf("Waiting for requests...\n");
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
