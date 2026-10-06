/// @file action_client.c
/// @brief C Fibonacci action client for the PURE BARE-METAL MPS2-AN385 — sends
///        one `order = 10` goal, prints the feedback and the result.
///
/// The same program as `examples/mps2-an385-freertos/c/action-client`, minus
/// what a board with no RTOS does not have (the `c/talker` sibling's file header
/// has the long form, issue 1512): no libc and no `printf` substitution, so
/// records are built by hand and emitted through `nros_log_emit_at`.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <nros/action.h>
#include <nros/app_main.h>
#include <nros/check.h>
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

// All nros state — and the feedback/result staging — in .bss, never on the
// reset stack.
static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_client_t action_client;
    nros_executor_t executor;
    example_interfaces_action_fibonacci_feedback fb;
    example_interfaces_action_fibonacci_result result;
    uint8_t result_buf[512];
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

/// Append `[a, b, …]`.
static size_t append_sequence(char* out, size_t cap, size_t pos, const int32_t* data,
                              uint32_t size) {
    pos = append_str(out, cap, pos, "[");
    for (uint32_t i = 0; i < size; i++) {
        if (i > 0) {
            pos = append_str(out, cap, pos, ", ");
        }
        pos = append_i64(out, cap, pos, data[i]);
    }
    return append_str(out, cap, pos, "]");
}

static void emit(nros_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    nros_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

static void emit_num(nros_log_severity_t severity, const char* text, int64_t value,
                     const char* file, uint32_t line) {
    char buf[96];
    size_t len = append_str(buf, sizeof(buf), 0u, text);
    len = append_i64(buf, sizeof(buf), len, value);
    buf[len] = '\0';
    emit(severity, buf, file, line);
}

static void feedback_callback(const nros_goal_uuid_t* goal_uuid, const uint8_t* feedback,
                              size_t feedback_len, void* context) {
    (void)goal_uuid;
    (void)context;
    if (example_interfaces_action_fibonacci_feedback_deserialize(&app.fb, feedback, feedback_len) !=
        0) {
        emit(NROS_LOG_SEVERITY_ERROR, "Failed to deserialize feedback", __FILE__, __LINE__);
        return;
    }
    // 64 terms of up to 11 characters plus separators: sized, not guessed.
    char line[64u * 13u + 48u];
    size_t len = append_str(line, sizeof(line), 0u, "Next number in sequence received: ");
    len = append_sequence(line, sizeof(line), len, app.fb.sequence.data, app.fb.sequence.size);
    line[len] = '\0';
    emit(NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

static void result_callback(const nros_goal_uuid_t* goal_uuid, nros_goal_status_t status,
                            const uint8_t* result, size_t result_len, void* context) {
    (void)goal_uuid;
    (void)status;
    (void)result;
    (void)result_len;
    (void)context;
}

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    nros_action_type_t fibonacci_type = {
        .type_name = example_interfaces_action_fibonacci_get_type_name(),
        .type_hash = example_interfaces_action_fibonacci_get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    // `NROS_ENTRY_LOCATOR` is the one producer of the compile-time connect
    // configuration (`<nros/entry_config.h>`, issue 0946); on this leaf it is
    // the empty bottom rung — baking `system.toml`'s locator is issue 1512's.
    NROS_CHECK_RET(
        nros_support_init(&app.support, NROS_ENTRY_LOCATOR, (uint8_t)NROS_ENTRY_DOMAIN_ID), 1);
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_client", "/", &app.support),
                   1);
    g_logger = nros_node_get_logger(&app.node);

    NROS_CHECK_RET(
        nros_action_client_init(&app.action_client, &app.node, "/fibonacci", &fibonacci_type), 1);
    NROS_SOFTCHECK(
        nros_action_client_set_feedback_callback(&app.action_client, feedback_callback, NULL));
    NROS_SOFTCHECK(
        nros_action_client_set_result_callback(&app.action_client, result_callback, NULL));
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(nros_executor_add_action_client(&app.executor, &app.action_client), 1);

    // Let discovery settle before the first goal, as the FreeRTOS sibling does.
    for (int i = 0; i < 300; i++) {
        rclc_executor_spin_some(&app.executor, 10000000ULL); // 10 ms
    }

    nros_ret_t ret = NROS_RET_OK;
    example_interfaces_action_fibonacci_goal goal;
    example_interfaces_action_fibonacci_goal_init(&goal);
    goal.order = 10;
    uint8_t goal_buf[64];
    size_t goal_len = 0;
    if (example_interfaces_action_fibonacci_goal_serialize(&goal, goal_buf, sizeof(goal_buf),
                                                           &goal_len) != 0) {
        emit(NROS_LOG_SEVERITY_ERROR, "Failed to serialize goal", __FILE__, __LINE__);
        ret = NROS_RET_ERROR;
        goto cleanup;
    }

    ret = nros_action_client_wait_for_action_server(&app.action_client, &app.executor, 10000);
    // issue 1686 — NROS_RET_UNSUPPORTED: this backend cannot see servers at
    // all (XRCE: the Agent owns the DDS graph), so the wait answered at once.
    // Send anyway; the request's own timeout is then the probe.
    if (ret != NROS_RET_OK && ret != NROS_RET_UNSUPPORTED) {
        emit_num(NROS_LOG_SEVERITY_ERROR, "Action server did not appear within 10s: ", ret,
                 __FILE__, __LINE__);
        goto cleanup;
    }

    emit(NROS_LOG_SEVERITY_INFO, "Sending goal", __FILE__, __LINE__);
    nros_goal_uuid_t goal_uuid;
    ret = nros_action_send_goal(&app.action_client, &app.executor, goal_buf, goal_len, &goal_uuid);
    if (ret != NROS_RET_OK) {
        emit_num(NROS_LOG_SEVERITY_ERROR,
                 ret == NROS_RET_REJECTED ? "Goal was rejected by server: "
                                          : "Failed to send goal: ",
                 ret, __FILE__, __LINE__);
        goto cleanup;
    }
    emit(NROS_LOG_SEVERITY_INFO, "Goal accepted by server, waiting for result", __FILE__, __LINE__);

    nros_goal_status_t final_status;
    size_t result_len = 0;
    ret = nros_action_get_result(&app.action_client, &app.executor, &goal_uuid, &final_status,
                                 app.result_buf, sizeof(app.result_buf), &result_len);
    if (ret == NROS_RET_OK) {
        if (example_interfaces_action_fibonacci_result_deserialize(&app.result, app.result_buf,
                                                                   result_len) == 0) {
            char line[64u * 13u + 32u];
            size_t len = append_str(line, sizeof(line), 0u, "Result received: ");
            len = append_sequence(line, sizeof(line), len, app.result.sequence.data,
                                  app.result.sequence.size);
            line[len] = '\0';
            emit(NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
        } else {
            emit(NROS_LOG_SEVERITY_ERROR, "Failed to deserialize result", __FILE__, __LINE__);
        }
    } else {
        emit_num(NROS_LOG_SEVERITY_ERROR, "Failed to get result: ", ret, __FILE__, __LINE__);
    }

cleanup:
    rclc_executor_fini(&app.executor);
    nros_action_client_fini(&app.action_client);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return (ret == NROS_RET_OK) ? 0 : 1;
}

NROS_APP_MAIN_REGISTER()
