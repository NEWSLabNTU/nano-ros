/// @file main.c
/// @brief C service client example (XRCE-DDS) - calls AddTwoInts service (blocking)

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/check.h>
#include <nros/client.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>

#include "example_interfaces.h"

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_client_t client;
    nros_executor_t executor;
} app;

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C Service Client - XRCE (AddTwoInts)\n");
    printf("============================================\n");

    const char* agent = getenv("XRCE_AGENT_ADDR");
    if (!agent) {
        agent = "127.0.0.1:2019";
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Agent: %s\n", agent);
    printf("Domain ID: %d\n", domain_id);

    memset(&app, 0, sizeof(app));

    nros_service_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    NROS_CHECK_RET(nros_support_init(&app.support, agent, domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_xrce_service_client", "/"), 1);
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    NROS_CHECK_RET(nros_client_init(&app.client, &app.node, &add_two_ints_type, "/add_two_ints"), 1);
    printf("Client created for service: %s\n", nros_client_get_service_name(&app.client));

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_client(&app.executor, &app.client), 1);
    nros_ret_t ret = NROS_RET_OK;

    struct {
        int64_t a;
        int64_t b;
    } test_cases[] = {{5, 3}, {10, 20}, {100, 200}, {-5, 10}};
    int num_cases = sizeof(test_cases) / sizeof(test_cases[0]);

    printf("\nCalling service %d times...\n\n", num_cases);

    int success_count = 0;

    for (int i = 0; i < num_cases; i++) {
        example_interfaces_srv_add_two_ints_request request;
        example_interfaces_srv_add_two_ints_request_init(&request);
        request.a = test_cases[i].a;
        request.b = test_cases[i].b;

        uint8_t req_buf[256];
        int32_t req_len = example_interfaces_srv_add_two_ints_request_serialize(&request, req_buf,
                                                                                sizeof(req_buf));
        if (req_len < 0) {
            fprintf(stderr, "Failed to serialize request\n");
            continue;
        }

        uint8_t resp_buf[256];
        size_t resp_len = 0;
        ret = nros_client_call(&app.client, req_buf, (size_t)req_len, resp_buf, sizeof(resp_buf),
                               &resp_len);

        if (ret == NROS_RET_OK) {
            example_interfaces_srv_add_two_ints_response response;
            if (example_interfaces_srv_add_two_ints_response_deserialize(&response, resp_buf,
                                                                         resp_len) == 0) {
                printf("Call [%d]: %lld + %lld = %lld", i + 1, (long long)request.a,
                       (long long)request.b, (long long)response.sum);
                if (response.sum == request.a + request.b) {
                    printf(" [OK]\n");
                    success_count++;
                } else {
                    printf(" [MISMATCH]\n");
                }
            } else {
                fprintf(stderr, "Call [%d]: Failed to deserialize response\n", i + 1);
            }
        } else if (ret == NROS_RET_TIMEOUT) {
            fprintf(stderr, "Call [%d]: Timeout (is the server running?)\n", i + 1);
        } else {
            fprintf(stderr, "Call [%d]: Failed with error %d\n", i + 1, ret);
        }
    }

    printf("\n%d/%d calls succeeded\n", success_count, num_cases);

    printf("\nShutting down...\n");
    nros_executor_fini(&app.executor);
    nros_client_fini(&app.client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return (success_count == num_cases) ? 0 : 1;
}
