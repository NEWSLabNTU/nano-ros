/// @file service_client.c
/// @brief C AddTwoInts service client for the PURE BARE-METAL MPS2-AN385 —
///        one `2 + 3` call, then the image ends.
///
/// The same program as `examples/mps2-an385-freertos/c/service-client`, minus
/// what a board with no RTOS does not have (the `c/talker` sibling's file header
/// has the long form, issue 1512): no libc — so no `argv` parsing with
/// `strtoll` — and no `printf` substitution, so records are built by hand and
/// emitted through `nros_log_emit_at`.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/client.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/log.h>
#include <nros/node.h>
#include <nros/rcl_compat.h>

#include "example_interfaces.h"

// Backend registration — the image's, exactly as on the cmake road (see the
// `c/talker` sibling for why it is written here rather than in `main.rs`).
extern int nros_rmw_zenoh_register(void);

void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

// All nros state in .bss, never on the reset stack.
static struct {
    nros_support_t support;
    nros_node_t node;
    nros_client_t client;
    nros_executor_t executor;
    example_interfaces_srv_add_two_ints_request request;
    example_interfaces_srv_add_two_ints_response response;
    uint8_t req_buf[256];
    uint8_t resp_buf[256];
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

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    nros_service_type_t add_two_ints_type = {
        .type_name = example_interfaces_srv_add_two_ints_get_type_name(),
        .type_hash = example_interfaces_srv_add_two_ints_get_type_hash(),
    };

    // `NROS_ENTRY_LOCATOR` is the one producer of the compile-time connect
    // configuration (`<nros/entry_config.h>`, issue 0946); on this leaf it is
    // the empty bottom rung — baking `system.toml`'s locator is issue 1512's.
    NROS_CHECK_RET(
        nros_support_init(&app.support, NROS_ENTRY_LOCATOR, (uint8_t)NROS_ENTRY_DOMAIN_ID), 1);
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "add_two_ints_client", "/", &app.support), 1);
    g_logger = nros_node_get_logger(&app.node);

    NROS_CHECK_RET(
        rclc_client_init_default(&app.client, &app.node, &add_two_ints_type, "/add_two_ints"), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_client(&app.executor, &app.client), 1);

    example_interfaces_srv_add_two_ints_request_init(&app.request);
    example_interfaces_srv_add_two_ints_response_init(&app.response);
    app.request.a = 2;
    app.request.b = 3;

    int exit_code = 0;
    nros_ret_t ret = example_interfaces_srv_add_two_ints_client_call(
        &app.client, &app.request, &app.response, app.req_buf, sizeof(app.req_buf), app.resp_buf,
        sizeof(app.resp_buf));
    if (ret == NROS_RET_OK) {
        char line[64];
        size_t len = append_str(line, sizeof(line), 0u, "Result of add_two_ints: ");
        len = append_i64(line, sizeof(line), len, app.response.sum);
        line[len] = '\0';
        emit(NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
    } else if (ret == NROS_RET_TIMEOUT) {
        emit(NROS_LOG_SEVERITY_ERROR, "Service call timed out (is the server running?)", __FILE__,
             __LINE__);
        exit_code = 1;
    } else {
        char line[64];
        size_t len = append_str(line, sizeof(line), 0u, "Service call failed with error ");
        len = append_i64(line, sizeof(line), len, (int64_t)ret);
        line[len] = '\0';
        emit(NROS_LOG_SEVERITY_ERROR, line, __FILE__, __LINE__);
        exit_code = 1;
    }

    example_interfaces_srv_add_two_ints_request_fini(&app.request);
    example_interfaces_srv_add_two_ints_response_fini(&app.response);
    rclc_executor_fini(&app.executor);
    nros_client_fini(&app.client);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return exit_code;
}

NROS_APP_MAIN_REGISTER()
