/// @file main.c
/// @brief C service client example - calls AddTwoInts service (blocking)

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/init.h>
#include <nros/node.h>
#include <nros/client.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
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

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    printf("nros C Service Client (AddTwoInts)\n");
    printf("=====================================\n");

    // Get configuration from environment
    const char* locator = getenv("ZENOH_LOCATOR");
    if (!locator) {
        locator = "tcp/127.0.0.1:7447";
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = 0;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Locator: %s\n", locator);
    printf("Domain ID: %d\n", domain_id);

    // Zero-initialize all static state
    memset(&app, 0, sizeof(app));

    // Build type info using generated type name/hash
    nros_message_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
        .serialized_size_max = 256,
    };

    // Initialize support context
    nros_ret_t ret = nros_support_init(&app.support, locator, domain_id);
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize support: %d\n", ret);
        return 1;
    }
    printf("Support initialized\n");

    // Create node
    ret = nros_node_init(&app.node, &app.support, "c_service_client", "/");
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    // Create service client
    ret = nros_client_init(
        &app.client,
        &app.node,
        &add_two_ints_type,
        "/add_two_ints"
    );
    if (ret != NROS_RET_OK) {
        fprintf(stderr, "Failed to initialize client: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return 1;
    }
    printf("Client created for service: %s\n",
           nros_client_get_service_name(&app.client));

    // Test cases: (a, b) pairs
    struct { int64_t a; int64_t b; } test_cases[] = {
        {5, 3}, {10, 20}, {100, 200}, {-5, 10}
    };
    int num_cases = sizeof(test_cases) / sizeof(test_cases[0]);

    printf("\nCalling service %d times...\n\n", num_cases);

    int success_count = 0;

    for (int i = 0; i < num_cases; i++) {
        // Prepare request using generated type
        example_interfaces_srv_add_two_ints_request request;
        example_interfaces_srv_add_two_ints_request_init(&request);
        request.a = test_cases[i].a;
        request.b = test_cases[i].b;

        // Serialize request using generated function
        uint8_t req_buf[256];
        int32_t req_len = example_interfaces_srv_add_two_ints_request_serialize(
            &request, req_buf, sizeof(req_buf));
        if (req_len < 0) {
            fprintf(stderr, "Failed to serialize request\n");
            continue;
        }

        // Call service (blocking)
        uint8_t resp_buf[256];
        size_t resp_len = 0;
        ret = nros_client_call(
            &app.client,
            req_buf, (size_t)req_len,
            resp_buf, sizeof(resp_buf),
            &resp_len
        );

        if (ret == NROS_RET_OK) {
            // Deserialize response using generated function
            example_interfaces_srv_add_two_ints_response response;
            if (example_interfaces_srv_add_two_ints_response_deserialize(
                    &response, resp_buf, resp_len) == 0) {
                printf("Call [%d]: %lld + %lld = %lld",
                       i + 1,
                       (long long)request.a,
                       (long long)request.b,
                       (long long)response.sum);

                if (response.sum == request.a + request.b) {
                    printf(" [OK]\n");
                    success_count++;
                } else {
                    printf(" [MISMATCH: expected %lld]\n",
                           (long long)(request.a + request.b));
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

    // Cleanup
    printf("\nShutting down...\n");
    nros_client_fini(&app.client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return (success_count == num_cases) ? 0 : 1;
}
