/// @file main.c
/// @brief C service client example — **callback** variant (RFC-0041 / Phase 239).
///
/// Mirrors the blocking `service-client` example, but receives each reply
/// through a `nros_response_callback_t` dispatched by `nros_executor_spin_some`
/// — the dual-mode alternative to `nros_client_call`. Send is non-blocking
/// (`nros_client_send_request_async`); the reply lands in the callback when the
/// executor next spins (the C analogue of rclcpp `async_send_request(req, cb)`).

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/client.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_client_t client;
    nros_executor_t executor;
    // Reply state shared with the callback.
    int reply_count;  // bumped each time the callback fires
    int64_t last_sum; // sum from the most recent reply
} app;

// ----------------------------------------------------------------------------
// Response callback — fired from `nros_executor_spin_some`, not a poll.
// ----------------------------------------------------------------------------

static void on_response(const uint8_t* response, size_t response_len, void* context) {
    (void)context;
    example_interfaces_srv_add_two_ints_response resp;
    if (example_interfaces_srv_add_two_ints_response_deserialize(&resp, response, response_len) ==
        0) {
        app.last_sum = resp.sum;
        app.reply_count++;
        printf("Response (callback): sum = %lld\n", (long long)resp.sum);
    } else {
        fprintf(stderr, "Callback: failed to deserialize response\n");
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline (Phase 177.34).
    setvbuf(stdout, NULL, _IOLBF, 0);

    printf("nros C Service Client (AddTwoInts, callback)\n");
    printf("=============================================\n");

    const char* locator = getenv("NROS_LOCATOR");
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

    memset(&app, 0, sizeof(app));

    nros_service_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    NROS_CHECK_RET(nros_support_init(&app.support, locator, domain_id), 1);
    printf("Support initialized\n");
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_service_client_callback", "/"), 1);
    printf("Node created: %s\n", nros_node_get_name(&app.node));

    NROS_CHECK_RET(nros_client_init(&app.client, &app.node, &add_two_ints_type, "/add_two_ints"),
                   1);
    printf("Client created for service: %s\n", nros_client_get_service_name(&app.client));

    // Phase 82: clients must be registered with an executor before use.
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_client(&app.executor, &app.client), 1);

    // RFC-0041: register the reply callback. It fires at spin, not via poll.
    NROS_CHECK_RET(nros_client_set_response_callback(&app.client, on_response, NULL), 1);
    printf("Response callback registered\n");

    // Let discovery settle (the callback client has no blocking call to gate on).
    for (int i = 0; i < 20; i++) {
        nros_executor_spin_some(&app.executor, 50ull * 1000 * 1000);
    }

    struct {
        int64_t a;
        int64_t b;
    } test_cases[] = {{5, 3}, {10, 20}, {100, 200}, {-5, 10}};
    int num_cases = sizeof(test_cases) / sizeof(test_cases[0]);

    printf("\nCalling service %d times (async + callback)...\n\n", num_cases);

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

        int before = app.reply_count;
        printf("Calling service: %lld + %lld = ?\n", (long long)request.a, (long long)request.b);

        nros_ret_t ret = nros_client_send_request_async(&app.client, req_buf, (size_t)req_len);
        if (ret != NROS_RET_OK) {
            fprintf(stderr, "Call [%d]: async send failed with error %d\n", i + 1, ret);
            continue;
        }

        // Spin until the reply callback fires (or a 5 s budget elapses).
        uint64_t waited_ms = 0;
        while (app.reply_count == before && waited_ms < 5000) {
            nros_executor_spin_some(&app.executor, 50ull * 1000 * 1000);
            waited_ms += 50;
        }

        if (app.reply_count > before) {
            int64_t expected = request.a + request.b;
            if (app.last_sum == expected) {
                printf("Call [%d]: OK (sum = %lld)\n", i + 1, (long long)app.last_sum);
                success_count++;
            } else {
                printf("Call [%d]: MISMATCH (got %lld, expected %lld)\n", i + 1,
                       (long long)app.last_sum, (long long)expected);
            }
        } else {
            fprintf(stderr, "Call [%d]: timeout waiting for callback\n", i + 1);
        }
    }

    printf("\n%d/%d callback calls succeeded\n", success_count, num_cases);

    printf("\nShutting down...\n");
    nros_executor_fini(&app.executor);
    nros_client_fini(&app.client);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

    printf("Goodbye!\n");
    return (success_count == num_cases) ? 0 : 1;
}

NROS_APP_MAIN_REGISTER_POSIX()
