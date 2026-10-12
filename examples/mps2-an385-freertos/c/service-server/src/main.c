/// @file main.c
/// @brief C service server example - AddTwoInts service using executor
///
/// phase-417 W5.e — the handler is TYPED. It receives a deserialized request
/// and an empty response to fill; it calls no CDR function, names no buffer
/// size, and allocates nothing. That is rclc's shape:
///
///     rclc: void (*rclc_service_callback_with_context_t)(const void *request,
///                                                        void *response,
///                                                        void *context)
///     ours: void (*..._handler_fn_t)(const ..._request *request,
///                                    ..._response *response, void *context)
///
/// — the same ownership (the CALLER owns both payload structs, so nothing on
/// the delivery path allocates), with the two pointers typed rather than
/// `void *`. The storage lives in the `..._service_handler_t` below, which is
/// where rclc's `request_msg` / `response_msg` arguments would have put it.
///
/// The byte-oriented callback this file used to carry is not gone — it is
/// `rclc_executor_add_service_raw()`, and it remains the only shape for a
/// caller doing its own CDR.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>

// nros modular includes (rclc-style)
#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/service.h>

// Generated C bindings for example_interfaces/srv/AddTwoInts
#include "example_interfaces.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

typedef struct {
    int request_count;
} server_context_t;

// Static allocation
static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_service_t service;
    rclc_executor_t executor;
    server_context_t ctx;
    // Caller-owned request + response storage and the typed callback. It must
    // outlive the service, so it lives here beside it.
    example_interfaces__srv__AddTwoInts__service_handler_t handler;
} app;

static volatile sig_atomic_t g_running = 1;
static rclc_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        rclc_executor_cancel(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Service callback - handle AddTwoInts request
// ----------------------------------------------------------------------------

static void service_callback(const example_interfaces__srv__AddTwoInts_Request* request,
                             example_interfaces__srv__AddTwoInts_Response* response,
                             void* context) {
    server_context_t* ctx = (server_context_t*)context;

    ctx->request_count++;

    printf("Incoming request\na: %lld b: %lld\n", (long long)request->a, (long long)request->b);

    response->sum = request->a + request->b;
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // Line-buffer stdout: glibc full-buffers non-tty stdout, so when piped to
    // a test harness each line must flush on its newline.
#ifdef _IOLBF /* absent on the bare-metal riscv64-threadx libc */
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    printf("nros C Service Server (AddTwoInts)\n");
    printf("=====================================\n");

    // Get configuration from environment
    const char* locator = getenv("NROS_LOCATOR");
    if (!locator) {
        locator = NROS_ENTRY_LOCATOR;
    }

    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = (uint8_t)NROS_ENTRY_DOMAIN_ID;
    if (domain_str) {
        domain_id = (uint8_t)atoi(domain_str);
    }

    printf("Locator: %s\n", locator);
    printf("Domain ID: %d\n", domain_id);

    // Zero-initialize all static state
    memset(&app, 0, sizeof(app));

    RCLC_CHECK_RET(rclc_support_init(&app.support, locator, domain_id), 1);
    printf("Support initialized\n");
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "add_two_ints_server", "/", &app.support), 1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    // Install the typed handler and create the service. The generated
    // `_service_init` names the type support and `_executor_add_service` binds
    // the trampoline to the handler, so neither can disagree with the type —
    // there is no hand-written `rclc_service_type_t` here any more.
    RCLC_CHECK_RET(example_interfaces__srv__AddTwoInts__service_handler_init(
                       &app.handler, service_callback, &app.ctx),
                   1);
    RCLC_CHECK_RET(
        example_interfaces__srv__AddTwoInts__service_init(&app.service, &app.node, "/add_two_ints"),
        1);
    printf("Service created: %s\n", rcl_service_get_service_name(&app.service));

    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    RCLC_CHECK_RET(example_interfaces__srv__AddTwoInts__executor_add_service(
                       &app.executor, &app.service, &app.handler),
                   1);
    printf("Executor created with %d handle(s)\n", rclc_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for service requests (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK && g_running) {
        fprintf(stderr, "Executor spin failed: %d\n", ret);
    }

    // Cleanup
    printf("\nShutting down...\n");
    printf("Total requests handled: %d\n", app.ctx.request_count);
    // A refused request never reaches the callback, so `request_count` alone
    // cannot report one. `error_count` is the other half.
    if (app.handler.error_count != 0u) {
        fprintf(stderr, "Requests refused: %u (last error %d)\n", app.handler.error_count,
                (int)app.handler.last_error);
    }
    rclc_executor_fini(&app.executor);
    rcl_service_fini(&app.service);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER()
