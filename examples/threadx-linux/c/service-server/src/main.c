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
/// `nros_executor_add_service_raw()`, and it remains the only shape for a
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
    nros_support_t support;
    nros_node_t node;
    nros_service_t service;
    nros_executor_t executor;
    server_context_t ctx;
    // Caller-owned request + response storage and the typed callback. It must
    // outlive the service, so it lives here beside it.
    example_interfaces_srv_add_two_ints_service_handler_t handler;
} app;

static volatile sig_atomic_t g_running = 1;
static nros_executor_t* g_executor = NULL;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nros_executor_cancel(g_executor);
    }
}

// ----------------------------------------------------------------------------
// Service callback - handle AddTwoInts request
// ----------------------------------------------------------------------------

static void service_callback(const example_interfaces_srv_add_two_ints_request* request,
                             example_interfaces_srv_add_two_ints_response* response,
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

    NROS_CHECK_RET(nros_support_init(&app.support, locator, domain_id), 1);
    printf("Support initialized\n");
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "add_two_ints_server", "/", &app.support), 1);
    printf("Node created: %s\n", rcl_node_get_name(&app.node));

    // Install the typed handler, then create the service against it. The
    // generated `_service_init` names the type support, the trampoline and the
    // handler together, so the three cannot disagree — there is no
    // hand-written `nros_service_type_t` here any more.
    NROS_CHECK_RET(example_interfaces_srv_add_two_ints_service_handler_init(
                       &app.handler, service_callback, &app.ctx),
                   1);
    NROS_CHECK_RET(example_interfaces_srv_add_two_ints_service_init(&app.service, &app.node,
                                                                    "/add_two_ints", &app.handler),
                   1);
    printf("Service created: %s\n", rcl_service_get_service_name(&app.service));

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    NROS_CHECK_RET(nros_executor_add_service(&app.executor, &app.service), 1);
    printf("Executor created with %d handle(s)\n", nros_executor_get_handle_count(&app.executor));

    // Set up signal handler
    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("\nWaiting for service requests (Ctrl+C to exit)...\n\n");

    // Spin with 100ms period
    nros_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
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
    nros_service_fini(&app.service);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);

    printf("Goodbye!\n");
    return 0;
}

NROS_APP_MAIN_REGISTER()
