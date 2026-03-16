/// @file main.c
/// @brief FreeRTOS C service client — calls AddTwoInts on /add_two_ints

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/client.h>

#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_client_t client;
} app;

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

void app_main(void) {
    printf("nros C Service Client (FreeRTOS)\n");

    memset(&app, 0, sizeof(app));

    nros_message_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
        .serialized_size_max = 256,
    };

    nros_ret_t ret = nros_support_init(&app.support, "tcp/192.0.3.1:7447", 0);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize support: %d\n", ret);
        return;
    }
    printf("Support initialized\n");

    ret = nros_node_init(&app.node, &app.support, "c_service_client", "/");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_client_init(&app.client, &app.node, &add_two_ints_type, "/add_two_ints");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize client: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    printf("Service client ready for /add_two_ints\n");

    struct { int64_t a; int64_t b; } test_cases[] = {
        {5, 3}, {10, 20}, {100, 200}, {-5, 10}
    };
    int num_cases = (int)(sizeof(test_cases) / sizeof(test_cases[0]));

    int success_count = 0;

    for (int i = 0; i < num_cases; i++) {
        example_interfaces_srv_add_two_ints_request request;
        example_interfaces_srv_add_two_ints_request_init(&request);
        request.a = test_cases[i].a;
        request.b = test_cases[i].b;

        uint8_t req_buf[256];
        int32_t req_len = example_interfaces_srv_add_two_ints_request_serialize(
                &request, req_buf, sizeof(req_buf));
        if (req_len < 0) {
            printf("Failed to serialize request\n");
            continue;
        }
        size_t req_size = (size_t)req_len;

        uint8_t resp_buf[256];
        size_t resp_len = 0;
        ret = nros_client_call(&app.client, req_buf, req_size,
                               resp_buf, sizeof(resp_buf), &resp_len);

        if (ret == NROS_RET_OK) {
            example_interfaces_srv_add_two_ints_response response;
            if (example_interfaces_srv_add_two_ints_response_deserialize(
                    &response, resp_buf, resp_len) == 0) {
                printf("Response: %lld + %lld = %lld\n",
                       (long long)request.a, (long long)request.b,
                       (long long)response.sum);
                if (response.sum == request.a + request.b) {
                    success_count++;
                }
            }
        } else if (ret == NROS_RET_TIMEOUT) {
            printf("Call [%d]: Timeout\n", i + 1);
        } else {
            printf("Call [%d]: Failed with error %d\n", i + 1, ret);
        }
    }

    printf("%d/%d calls succeeded\n", success_count, num_cases);
    printf("All service calls completed.\n");

    nros_client_fini(&app.client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}
