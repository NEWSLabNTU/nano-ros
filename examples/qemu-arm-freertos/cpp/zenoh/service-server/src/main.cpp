/// @file main.cpp
/// @brief C++ service server — AddTwoInts (FreeRTOS QEMU)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Service Server (FreeRTOS)\n");
    NROS_TRY_RET(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_service_server"), 1);
    printf("Node created\n");

    nros::Service<example_interfaces::srv::AddTwoInts> srv;
    NROS_TRY_RET(node.create_service(srv, "/add_two_ints"), 1);

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

NROS_APP_MAIN_REGISTER_VOID()
