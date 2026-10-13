/// @file main.c
/// @brief C service client example — **callback** variant.
///
/// Mirrors the blocking `service-client` example, but receives each reply
/// through a typed response callback dispatched by `rclc_executor_spin_some`
/// — the dual-mode alternative to `rclc_client_call`. Send is non-blocking
/// (`..._client_send_request`); the reply lands in the callback when the
/// executor next spins (the C analogue of rclcpp `async_send_request(req, cb)`).
///
/// phase-417 W5.e — the callback is TYPED, which is rclc's shape:
///
///     rclc: void (*rclc_client_callback_t)(const void *response)
///     ours: void (*..._response_fn_t)(const ..._response *response,
///                                     void *context)
///
/// The response storage is the CALLER's — it lives in the
/// `..._client_handler_t` below — so nothing on the reply path allocates. A
/// reply whose CDR does not decode does NOT reach the callback: handing it a
/// zeroed struct would be indistinguishable from a server that answered with
/// defaults.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// The C API at rcl/rclc's include paths
#include <rclc/client.h>
#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_client_t client;
    rclc_executor_t executor;
    // Caller-owned response storage + the typed callback. Must outlive the
    // client, so it lives here beside it.
    example_interfaces__srv__AddTwoInts__client_handler_t handler;
    // Reply state shared with the callback.
    int reply_count; // bumped each time the callback fires
} app;

// ----------------------------------------------------------------------------
// Response callback — fired from `rclc_executor_spin_some`, not a poll.
// ----------------------------------------------------------------------------

static void on_response(const example_interfaces__srv__AddTwoInts_Response* response,
                        void* context) {
    (void)context;
    app.reply_count++;
    printf("Result of add_two_ints: %lld\n", (long long)response->sum);
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    printf("nros C Service Client (AddTwoInts, callback)\n");
    printf("=============================================\n");

    // Operands from the first two positional args (default: 2 3).
    int64_t a = 2;
    int64_t b = 3;
    if (argc >= 3) {
        char* end_a = NULL;
        char* end_b = NULL;
        long long parsed_a = strtoll(argv[1], &end_a, 10);
        long long parsed_b = strtoll(argv[2], &end_b, 10);
        if (end_a && *end_a == '\0' && end_b && *end_b == '\0') {
            a = (int64_t)parsed_a;
            b = (int64_t)parsed_b;
        }
    }

    memset(&app, 0, sizeof(app));

    rclc_service_type_t add_two_ints_type = {
        .type_name = example_interfaces__srv__AddTwoInts__get_type_name(),
        .type_hash = example_interfaces__srv__AddTwoInts__get_type_hash(),
    };

    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    printf("Support initialized\n");
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "add_two_ints_client_cb", "/", &app.support),
                   1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    RCLC_CHECK_RET(
        rclc_client_init_default(&app.client, &app.node, &add_two_ints_type, "/add_two_ints"), 1);
    printf("Client created for service: %s\n", rcl_client_get_service_name(&app.client));

    // Clients must be registered with an executor before use.
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_attach_client(&app.executor, &app.client), 1);

    // Register the reply callback. It fires at spin, not via poll.
    RCLC_CHECK_RET(
        example_interfaces__srv__AddTwoInts__client_handler_init(&app.handler, on_response, NULL),
        1);
    RCLC_CHECK_RET(example_interfaces__srv__AddTwoInts__client_set_response_callback(&app.client,
                                                                                     &app.handler),
                   1);
    printf("Response callback registered\n");

    // Let discovery settle (the callback client has no blocking call to gate on).
    for (int i = 0; i < 20; i++) {
        rclc_executor_spin_some(&app.executor, 50ull * 1000 * 1000);
    }

    int exit_code = 0;

    example_interfaces__srv__AddTwoInts_Request request;
    memset(&request, 0, sizeof(request));
    request.a = a;
    request.b = b;

    // Caller-owned scratch for the request's wire form; borrowed for the send.
    uint8_t req_buf[256];
    {
        rcl_ret_t ret = example_interfaces__srv__AddTwoInts__client_send_request(
            &app.client, &request, req_buf, sizeof(req_buf));
        if (ret != RCL_RET_OK) {
            fprintf(stderr, "Async send failed with error %d\n", ret);
            exit_code = 1;
        } else {
            // Spin until the reply callback fires (or a 5 s budget elapses).
            uint64_t waited_ms = 0;
            while (app.reply_count == 0 && waited_ms < 5000) {
                rclc_executor_spin_some(&app.executor, 50ull * 1000 * 1000);
                waited_ms += 50;
            }

            if (app.reply_count == 0) {
                fprintf(stderr, "Timeout waiting for callback\n");
                exit_code = 1;
            }
        }
    }
    // A reply that failed to decode never reaches the callback, so a zero
    // `reply_count` alone cannot tell the two apart. `error_count` can.
    if (app.handler.error_count != 0u) {
        fprintf(stderr, "Replies refused: %u (last error %d)\n", app.handler.error_count,
                (int)app.handler.last_error);
    }

    printf("\nShutting down...\n");
    rclc_executor_fini(&app.executor);
    rcl_client_fini(&app.client, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return exit_code;
}
