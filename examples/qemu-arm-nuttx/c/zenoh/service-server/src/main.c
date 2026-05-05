/// @file main.c
/// @brief NuttX C service server example - AddTwoInts service

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>

#include <nros/app_config.h>
#include "example_interfaces.h"

// NuttX embedded config — matches board crate defaults (server = 192.0.3.10)
#ifndef NROS_APP_CONFIG.zenoh.locator
#define NROS_APP_CONFIG.zenoh.locator "tcp/192.0.3.1:7447"
#endif
#ifndef NROS_APP_CONFIG.zenoh.domain_id
#define NROS_APP_CONFIG.zenoh.domain_id 0
#endif

typedef struct {
    int request_count;
} server_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_service_t service;
    nros_executor_t executor;
    server_context_t ctx;
} app;

static bool service_callback(const uint8_t* request_data,
                             size_t request_len,
                             uint8_t* response_data,
                             size_t response_capacity,
                             size_t* response_len,
                             void* context) {
    server_context_t* ctx = (server_context_t*)context;

    example_interfaces_srv_add_two_ints_request request;
    if (example_interfaces_srv_add_two_ints_request_deserialize(
            &request, request_data, request_len) != 0) {
        fprintf(stderr, "Failed to deserialize request\n");
        return false;
    }

    ctx->request_count++;

    example_interfaces_srv_add_two_ints_response response;
    example_interfaces_srv_add_two_ints_response_init(&response);
    response.sum = request.a + request.b;

    printf("Request [%d]: %lld + %lld = %lld\n",
           ctx->request_count,
           (long long)request.a,
           (long long)request.b,
           (long long)response.sum);
    fflush(stdout);

    int32_t len = example_interfaces_srv_add_two_ints_response_serialize(
        &response, response_data, response_capacity);
    if (len < 0) {
        fprintf(stderr, "Failed to serialize response\n");
        return false;
    }

    *response_len = (size_t)len;
    return true;
}

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;


    printf("nros NuttX C Service Server (AddTwoInts)\n");
    printf("Locator: %s\n", NROS_APP_CONFIG.zenoh.locator);

    memset(&app, 0, sizeof(app));

    // Re-seed /dev/urandom with a per-example unique value. NuttX's
    // xorshift128 PRNG starts with a fixed seed, so two QEMU instances
    // otherwise generate identical Zenoh session IDs and zenohd rejects
    // the second connection with MAX_LINKS. Writing bytes to /dev/urandom
    // reseeds the PRNG state.
    {
        FILE* urandom = fopen("/dev/urandom", "wb");
        if (urandom != NULL) {
            const uint8_t seed[4] = {10, 0, 2, 32};
            fwrite(seed, 1, sizeof(seed), urandom);
            fclose(urandom);
        }
    }

    nros_service_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    // Wait for NuttX networking to become ready before attempting the
    // zenoh TCP session. NuttX's poll()/select() don't cooperate with
    // blocking connect() well enough to rely on connect_timeout, so we
    // just sleep for a few seconds after boot and let the virtio-net
    // driver + DHCP/static IP setup finish. Mirrors the 5-second wait
    // in packages/boards/nros-board-nuttx-qemu-arm/src/node.rs::run().
    fflush(stdout);
    sleep(5);

    NROS_CHECK_RET(nros_support_init(&app.support, NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "nuttx_c_service_server", "/"), 1);
    NROS_CHECK_RET(nros_service_init(
        &app.service, &app.node, &add_two_ints_type,
        "/add_two_ints", service_callback, &app.ctx), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_SOFTCHECK(nros_executor_add_service(&app.executor, &app.service));

    printf("Waiting for requests...\n\n");
    // NuttX libc full-buffers stdout under the test harness's pipe.
    // See action-server for rationale.
    fflush(stdout);
    nros_executor_spin_period(&app.executor, 100000000ULL);

    nros_executor_fini(&app.executor);
    nros_service_fini(&app.service);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

}

NROS_APP_MAIN_REGISTER_VOID()
