/// @file service_server.c
/// @brief C AddTwoInts service server for the PURE BARE-METAL MPS2-AN385.
///
/// The same program as `examples/mps2-an385-freertos/c/service-server`, minus
/// what a board with no RTOS does not have (the `c/talker` sibling's file header
/// has the long form, issue 1512): no libc and no `printf` substitution, so
/// records are built by hand and emitted through `nros_log_emit_at`.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/log.h>
#include <nros/node.h>
#include <nros/rcl_compat.h>
#include <nros/service.h>

#include "example_interfaces.h"

// Backend registration — the image's, exactly as on the cmake road (see the
// `c/talker` sibling for why it is written here rather than in `main.rs`).
extern int nros_rmw_zenoh_register(void);

void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

typedef struct {
    uint32_t request_count;
} server_context_t;

// All nros state in .bss, never on the reset stack.
static struct {
    nros_support_t support;
    nros_node_t node;
    nros_service_t service;
    nros_executor_t executor;
    server_context_t ctx;
    example_interfaces_srv_add_two_ints_service_handler_t handler;
} app;

static nros_logger_t g_logger;

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

static void emit(nros_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    nros_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

// Typed service callback: the generated handler decodes the request and encodes
// the reply, so this body sees fields only.
static void service_callback(const example_interfaces_srv_add_two_ints_request* request,
                             example_interfaces_srv_add_two_ints_response* response,
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
    emit(NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // `NROS_ENTRY_LOCATOR` is the one producer of the compile-time connect
    // configuration (`<nros/entry_config.h>`, issue 0946); on this leaf it is
    // the empty bottom rung — baking `system.toml`'s locator is issue 1512's.
    NROS_CHECK_RET(
        nros_support_init(&app.support, NROS_ENTRY_LOCATOR, (uint8_t)NROS_ENTRY_DOMAIN_ID), 1);
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "add_two_ints_server", "/", &app.support), 1);
    g_logger = nros_node_get_logger(&app.node);

    NROS_CHECK_RET(example_interfaces_srv_add_two_ints_service_handler_init(
                       &app.handler, service_callback, &app.ctx),
                   1);
    NROS_CHECK_RET(example_interfaces_srv_add_two_ints_service_init(&app.service, &app.node,
                                                                    "/add_two_ints", &app.handler),
                   1);

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_service(&app.executor, &app.service), 1);

    // Spin forever: `app_main` returning is the end of the image.
    nros_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK) {
        emit(NROS_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    nros_service_fini(&app.service);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER()
