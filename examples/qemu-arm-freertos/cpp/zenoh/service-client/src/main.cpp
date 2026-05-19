/// @file main.cpp
/// @brief C++ service client — AddTwoInts (FreeRTOS QEMU, async Future)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>
#include <nros/app_config.h>
#include "example_interfaces.hpp"

// Phase 88.16.H — set after `nros::create_node`; used by post-init
// diagnostics. nullptr before init = `NROS_LOG_*` silently drops.
static nros_logger_t g_logger = nullptr;

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C++ Service Client (FreeRTOS)\n");
    NROS_TRY_RET(nros::init(NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_service_client"), 1);
    g_logger = node.get_logger();
    nros_log_init();
    printf("Node created\n");

    nros::Client<example_interfaces::srv::AddTwoInts> client;
    NROS_TRY_RET(node.create_client(client, "/add_two_ints"), 1);
    nros::Result ret;

    printf("Service client ready\n");
    struct { int64_t a, b; } cases[] = {{5,3},{10,20},{100,200},{-5,10}};
    int ok_count = 0;
    for (int i = 0; i < 4; i++) {
        example_interfaces::srv::AddTwoInts::Request req;
        req.a = cases[i].a; req.b = cases[i].b;
        example_interfaces::srv::AddTwoInts::Response resp;
        auto fut = client.send_request(req);
        if (fut.is_consumed()) { NROS_LOG_INFO(g_logger, "Call [%d] send failed", i+1); continue; }
        ret = fut.wait(nros::global_handle(), 5000, resp);
        if (ret.ok()) {
            printf("Response: %d + %d = %d", (int)req.a, (int)req.b, (int)resp.sum);
            if (resp.sum == req.a + req.b) { printf(" [OK]\n"); ok_count++; }
            else printf(" [MISMATCH]\n");
        } else {
            NROS_LOG_INFO(g_logger, "Call [%d] failed: %d", i+1, ret.raw());
        }
    }
    NROS_LOG_INFO(g_logger, "All service calls completed (%d/%d succeeded)", ok_count, 4);
    nros::shutdown();
    return 0;
}
NROS_APP_MAIN_REGISTER_VOID()
