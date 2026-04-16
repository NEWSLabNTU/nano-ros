/// @file main.c
/// @brief NuttX C service client example - calls AddTwoInts service

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/client.h>
#include <nros/executor.h>

#include "example_interfaces.h"

// NuttX embedded config — matches board crate defaults (client = 192.0.3.11)
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_client_t client;
    nros_executor_t executor;
} app;

void app_main(void) {

    printf("nros NuttX C Service Client (AddTwoInts)\n");
    printf("Locator: %s\n", APP_ZENOH_LOCATOR);

    memset(&app, 0, sizeof(app));

    // Re-seed /dev/urandom with a per-example unique value. NuttX's
    // xorshift128 PRNG starts with a fixed seed, so two QEMU instances
    // otherwise generate identical Zenoh session IDs and zenohd rejects
    // the second connection with MAX_LINKS. Writing bytes to /dev/urandom
    // reseeds the PRNG state.
    {
        FILE* urandom = fopen("/dev/urandom", "wb");
        if (urandom != NULL) {
            const uint8_t seed[4] = {10, 0, 2, 33};
            fwrite(seed, 1, sizeof(seed), urandom);
            fclose(urandom);
        }
    }

    nros_message_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
        .serialized_size_max = 256,
    };

    // Wait for NuttX networking to become ready before attempting the
    // zenoh TCP session. NuttX's poll()/select() don't cooperate with
    // blocking connect() well enough to rely on connect_timeout, so we
    // just sleep for a few seconds after boot and let the virtio-net
    // driver + DHCP/static IP setup finish. Mirrors the 5-second wait
    // in packages/boards/nros-nuttx-qemu-arm/src/node.rs::run().
    fflush(stdout);
    sleep(5);

    nros_ret_t ret = nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return;
    }

    ret = nros_node_init(&app.node, &app.support, "nuttx_c_service_client", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_client_init(&app.client, &app.node, &add_two_ints_type, "/add_two_ints");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize client: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize executor: %d\n", ret);
        nros_client_fini(&app.client);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_executor_add_client(&app.executor, &app.client);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to register client with executor: %d\n", ret);
        nros_executor_fini(&app.executor);
        nros_client_fini(&app.client);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    struct { int64_t a; int64_t b; } test_cases[] = {
        {5, 3}, {10, 20}, {100, 200}, {-5, 10}
    };
    int num_cases = sizeof(test_cases) / sizeof(test_cases[0]);
    int success_count = 0;

    printf("Calling service %d times...\n\n", num_cases);

    for (int i = 0; i < num_cases; i++) {
        example_interfaces_srv_add_two_ints_request request;
        example_interfaces_srv_add_two_ints_request_init(&request);
        request.a = test_cases[i].a;
        request.b = test_cases[i].b;

        uint8_t req_buf[256];
        int32_t req_len = example_interfaces_srv_add_two_ints_request_serialize(
            &request, req_buf, sizeof(req_buf));
        if (req_len < 0) {
            fprintf(stderr, "Failed to serialize request\n");
            continue;
        }

        uint8_t resp_buf[256];
        size_t resp_len = 0;
        ret = nros_client_call(&app.client,
            req_buf, (size_t)req_len,
            resp_buf, sizeof(resp_buf), &resp_len);

        if (ret == NROS_RET_OK) {
            example_interfaces_srv_add_two_ints_response response;
            if (example_interfaces_srv_add_two_ints_response_deserialize(
                    &response, resp_buf, resp_len) == 0) {
                printf("Call [%d]: %lld + %lld = %lld\n",
                       i + 1,
                       (long long)request.a,
                       (long long)request.b,
                       (long long)response.sum);
                success_count++;
            }
        } else {
            fprintf(stderr, "Call [%d]: Failed with error %d\n", i + 1, ret);
        }
    }

    printf("\n%d/%d calls succeeded\n", success_count, num_cases);

    nros_executor_fini(&app.executor);
    nros_client_fini(&app.client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return (success_count == num_cases) ? 0 : 1;
}
