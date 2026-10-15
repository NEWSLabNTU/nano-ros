/// @file service_server.c
/// @brief C AddTwoInts service server for the PURE BARE-METAL MPS2-AN385.
///
/// The same program as `examples/mps2-an385-freertos/c/service-server`, minus
/// what a board with no RTOS does not have (the `c/talker` sibling's file header
/// has the long form, issue 1512): no libc and no `printf` substitution, so
/// records are built by hand and emitted through `rclc_log_emit_at`.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rcl/rcl.h>
#include <rclc/service.h>

#include "example_interfaces.h"

typedef struct {
    uint32_t request_count;
} server_context_t;

// All nros state in .bss, never on the reset stack.
static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_service_t service;
    rclc_executor_t executor;
    server_context_t ctx;
    example_interfaces__srv__AddTwoInts__service_handler_t handler;
} app;

static rclc_logger_t g_logger;

// Hand-rolled formatting: each takes the buffer's CAPACITY, truncates rather
// than run past it, and leaves room for the caller's NUL.

static size_t append_str(char* out, size_t cap, size_t pos, const char* text) {
    while (*text != '\0' && pos + 1u < cap) {
        out[pos++] = *text++;
    }
    return pos;
}

/// Append `value` in decimal. Negated through its unsigned magnitude, so
/// INT64_MIN does not overflow.
static size_t append_i64(char* out, size_t cap, size_t pos, int64_t value) {
    uint64_t magnitude = (uint64_t)value;
    if (value < 0) {
        if (pos + 1u < cap) {
            out[pos++] = '-';
        }
        magnitude = 0u - magnitude;
    }
    char digits[20];
    size_t n = 0;
    do {
        digits[n++] = (char)('0' + (magnitude % 10u));
        magnitude /= 10u;
    } while (magnitude != 0u && n < sizeof(digits));
    while (n > 0u && pos + 1u < cap) {
        out[pos++] = digits[--n];
    }
    return pos;
}

static void emit(rcl_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    rclc_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

// Typed service callback: the generated handler decodes the request and encodes
// the reply, so this body sees fields only.
static void service_callback(const example_interfaces__srv__AddTwoInts_Request* request,
                             example_interfaces__srv__AddTwoInts_Response* response,
                             void* context) {
    server_context_t* ctx = (server_context_t*)context;
    ctx->request_count++;
    response->sum = request->a + request->b;

    char line[96];
    size_t len = append_str(line, sizeof(line), 0u, "Incoming request a: ");
    len = append_i64(line, sizeof(line), len, request->a);
    len = append_str(line, sizeof(line), len, " b: ");
    len = append_i64(line, sizeof(line), len, request->b);
    line[len] = '\0';
    emit(RCLC_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // `rclc_support_init` takes the connect configuration from the build
    // (`<rclc/detail/entry_config.h>`, issue 0946); on this leaf that is the empty
    // bottom rung, and baking `system.toml`'s locator is issue 1512's.
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "add_two_ints_server", "/", &app.support), 1);
    g_logger = rclc_node_get_logger(&app.node);

    RCLC_CHECK_RET(example_interfaces__srv__AddTwoInts__service_handler_init(
                       &app.handler, service_callback, &app.ctx),
                   1);
    RCLC_CHECK_RET(
        example_interfaces__srv__AddTwoInts__service_init(&app.service, &app.node, "/add_two_ints"),
        1);

    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    RCLC_CHECK_RET(example_interfaces__srv__AddTwoInts__executor_add_service(
                       &app.executor, &app.service, &app.handler),
                   1);

    // The readiness marker the harness waits on (`expect_ready`, phase-342 W7).
    emit(RCLC_LOG_SEVERITY_INFO, "Waiting for service requests", __FILE__, __LINE__);

    // Spin forever: `app_main` returning is the end of the image.
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK) {
        emit(RCLC_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    rcl_service_fini(&app.service, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}
