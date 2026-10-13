/// @file action_client.c
/// @brief C Fibonacci action client for the PURE BARE-METAL MPS2-AN385 — sends
///        one `order = 10` goal, prints the feedback and the result.
///
/// The same program as `examples/mps2-an385-freertos/c/action-client`, minus
/// what a board with no RTOS does not have (the `c/talker` sibling's file header
/// has the long form, issue 1512): no libc and no `printf` substitution, so
/// records are built by hand and emitted through `rclc_log_emit_at`.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <rclc/action_client.h>
#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rcl/rcl.h>

#include "example_interfaces.h"

// All nros state — and the feedback/result staging — in .bss, never on the
// reset stack.
static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_action_client_t action_client;
    rclc_executor_t executor;
    example_interfaces__action__Fibonacci_Feedback fb;
    example_interfaces__action__Fibonacci_Result result;
    // The bytes behind `fb.sequence` / `result.sequence` (phase-492 W7): the
    // deserializer writes into them and refuses a longer sequence.
    example_interfaces__action__Fibonacci_Feedback__Storage fb_storage;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    uint8_t result_buf[512];
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

static void emit(rcl_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    rclc_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

static void emit_num(rcl_log_severity_t severity, const char* text, int64_t value, const char* file,
                     uint32_t line) {
    char buf[96];
    size_t len = append_str(buf, sizeof(buf), 0u, text);
    len = append_i64(buf, sizeof(buf), len, value);
    buf[len] = '\0';
    emit(severity, buf, file, line);
}

static void feedback_callback(const rclc_goal_uuid_t* goal_uuid, const uint8_t* feedback,
                              size_t feedback_len, void* context) {
    (void)goal_uuid;
    (void)context;
    if (example_interfaces__action__Fibonacci_Feedback__deserialize(&app.fb, feedback,
                                                                    feedback_len) != 0) {
        emit(RCLC_LOG_SEVERITY_ERROR, "Failed to deserialize feedback", __FILE__, __LINE__);
        return;
    }
    // 64 terms of up to 11 characters plus separators: sized, not guessed.
    char line[64u * 13u + 48u];
    size_t len = append_str(line, sizeof(line), 0u, "Next number in sequence received: ");
    len = append_sequence(line, sizeof(line), len, app.fb.sequence.data, app.fb.sequence.size);
    line[len] = '\0';
    emit(RCLC_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

static void result_callback(const rclc_goal_uuid_t* goal_uuid, rclc_goal_status_t status,
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

    rclc_action_type_t fibonacci_type = {
        .type_name = example_interfaces__action__Fibonacci__get_type_name(),
        .type_hash = example_interfaces__action__Fibonacci__get_type_hash(),
        .goal_serialized_size_max = 8,
        .result_serialized_size_max = 264,
        .feedback_serialized_size_max = 264,
    };

    // `rclc_support_init` takes the connect configuration from the build
    // (`<nros/entry_config.h>`, issue 0946); on this leaf that is the empty
    // bottom rung, and baking `system.toml`'s locator is issue 1512's.
    example_interfaces__action__Fibonacci_Feedback__init_in(&app.fb, &app.fb_storage);
    example_interfaces__action__Fibonacci_Result__init_in(&app.result, &app.result_storage);
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_client", "/", &app.support),
                   1);
    g_logger = rclc_node_get_logger(&app.node);

    RCLC_CHECK_RET(
        rcl_action_client_init(&app.action_client, &app.node, &fibonacci_type, "/fibonacci", NULL),
        1);
    RCLC_SOFTCHECK(
        rclc_action_client_set_feedback_callback(&app.action_client, feedback_callback, NULL));
    RCLC_SOFTCHECK(
        rclc_action_client_set_result_callback(&app.action_client, result_callback, NULL));
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_attach_action_client(&app.executor, &app.action_client), 1);

    // Let discovery settle before the first goal, as the FreeRTOS sibling does.
    for (int i = 0; i < 300; i++) {
        rclc_executor_spin_some(&app.executor, 10000000ULL); // 10 ms
    }

    rcl_ret_t ret = RCL_RET_OK;
    example_interfaces__action__Fibonacci_Goal goal;
    memset(&goal, 0, sizeof(goal));
    goal.order = 10;
    uint8_t goal_buf[64];
    size_t goal_len = 0;
    if (example_interfaces__action__Fibonacci_Goal__serialize(&goal, goal_buf, sizeof(goal_buf),
                                                              &goal_len) != 0) {
        emit(RCLC_LOG_SEVERITY_ERROR, "Failed to serialize goal", __FILE__, __LINE__);
        ret = RCL_RET_ERROR;
        goto cleanup;
    }

    ret = rclc_action_client_wait_for_action_server(&app.action_client, &app.executor, 10000);
    // issue 1686 — RCL_RET_UNSUPPORTED: this backend cannot see servers at
    // all (XRCE: the Agent owns the DDS graph), so the wait answered at once.
    // Send anyway; the request's own timeout is then the probe.
    if (ret != RCL_RET_OK && ret != RCL_RET_UNSUPPORTED) {
        emit_num(RCLC_LOG_SEVERITY_ERROR, "Action server did not appear within 10s: ", ret,
                 __FILE__, __LINE__);
        goto cleanup;
    }

    emit(RCLC_LOG_SEVERITY_INFO, "Sending goal", __FILE__, __LINE__);
    rclc_goal_uuid_t goal_uuid;
    ret = rclc_action_send_goal(&app.action_client, &app.executor, goal_buf, goal_len, &goal_uuid);
    if (ret != RCL_RET_OK) {
        emit_num(RCLC_LOG_SEVERITY_ERROR,
                 ret == RCLC_RET_REJECTED ? "Goal was rejected by server: "
                                          : "Failed to send goal: ",
                 ret, __FILE__, __LINE__);
        goto cleanup;
    }
    emit(RCLC_LOG_SEVERITY_INFO, "Goal accepted by server, waiting for result", __FILE__, __LINE__);

    rclc_goal_status_t final_status;
    size_t result_len = 0;
    ret = rclc_action_get_result(&app.action_client, &app.executor, &goal_uuid, &final_status,
                                 app.result_buf, sizeof(app.result_buf), &result_len);
    if (ret == RCL_RET_OK) {
        if (example_interfaces__action__Fibonacci_Result__deserialize(&app.result, app.result_buf,
                                                                      result_len) == 0) {
            char line[64u * 13u + 32u];
            size_t len = append_str(line, sizeof(line), 0u, "Result received: ");
            len = append_sequence(line, sizeof(line), len, app.result.sequence.data,
                                  app.result.sequence.size);
            line[len] = '\0';
            emit(RCLC_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
        } else {
            emit(RCLC_LOG_SEVERITY_ERROR, "Failed to deserialize result", __FILE__, __LINE__);
        }
    } else {
        emit_num(RCLC_LOG_SEVERITY_ERROR, "Failed to get result: ", ret, __FILE__, __LINE__);
    }

cleanup:
    rclc_executor_fini(&app.executor);
    rcl_action_client_fini(&app.action_client, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return (ret == RCL_RET_OK) ? 0 : 1;
}

NROS_APP_MAIN_REGISTER()
