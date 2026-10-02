/// @file action_server.c
/// @brief C Fibonacci action server for the PURE BARE-METAL MPS2-AN385.
///
/// The same program as `examples/mps2-an385-freertos/c/action-server`, minus
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

typedef struct {
    uint32_t goal_count;
    int32_t accepted_order;
} server_context_t;

// All nros state — and the feedback/result staging, which is the largest thing
// this image holds per goal — in .bss, never on the reset stack.
static struct {
    nros_support_t support;
    nros_node_t node;
    nros_action_server_t action_server;
    nros_executor_t executor;
    server_context_t ctx;
    example_interfaces_action_fibonacci_feedback fb;
    example_interfaces_action_fibonacci_result result;
    uint8_t fb_buf[512];
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

static void emit(nros_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    nros_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

/// One line of fixed text followed by a number.
static void emit_num(nros_log_severity_t severity, const char* text, int64_t value,
                     const char* file, uint32_t line) {
    char buf[96];
    size_t len = append_str(buf, sizeof(buf), 0u, text);
    len = append_i64(buf, sizeof(buf), len, value);
    buf[len] = '\0';
    emit(severity, buf, file, line);
}

static nros_goal_response_t goal_callback(nros_action_server_t* server,
                                          const nros_goal_handle_t* goal,
                                          const uint8_t* goal_request, size_t goal_len,
                                          void* context) {
    (void)server;
    (void)goal;
    example_interfaces_action_fibonacci_goal goal_msg;
    if (example_interfaces_action_fibonacci_goal_deserialize(&goal_msg, goal_request, goal_len) !=
        0) {
        emit(NROS_LOG_SEVERITY_ERROR, "Failed to deserialize goal", __FILE__, __LINE__);
        return NROS_GOAL_REJECT;
    }
    emit_num(NROS_LOG_SEVERITY_INFO, "Received goal request with order ", goal_msg.order, __FILE__,
             __LINE__);
    // The sequence holds 64 terms; anything that would overrun it is refused.
    if (goal_msg.order < 0 || goal_msg.order >= 64) {
        emit(NROS_LOG_SEVERITY_WARN, "Goal rejected: order out of range", __FILE__, __LINE__);
        return NROS_GOAL_REJECT;
    }
    server_context_t* ctx = (server_context_t*)context;
    if (ctx != NULL) {
        ctx->accepted_order = goal_msg.order;
    }
    return NROS_GOAL_ACCEPT_AND_EXECUTE;
}

static nros_cancel_response_t cancel_callback(nros_action_server_t* server,
                                              const nros_goal_handle_t* goal, void* context) {
    (void)server;
    (void)goal;
    (void)context;
    emit(NROS_LOG_SEVERITY_INFO, "Cancel request for goal", __FILE__, __LINE__);
    return NROS_CANCEL_ACCEPT;
}

static void accepted_callback(nros_action_server_t* server, const nros_goal_handle_t* goal,
                              void* context) {
    server_context_t* ctx = (server_context_t*)context;
    ctx->goal_count++;
    emit(NROS_LOG_SEVERITY_INFO, "Executing goal", __FILE__, __LINE__);

    int32_t order = ctx->accepted_order;
    nros_ret_t ret = nros_action_execute(server, goal);
    if (ret != NROS_RET_OK) {
        emit_num(NROS_LOG_SEVERITY_ERROR, "Failed to set executing state: ", ret, __FILE__,
                 __LINE__);
        return;
    }

    example_interfaces_action_fibonacci_feedback* fb = &app.fb;
    example_interfaces_action_fibonacci_feedback_init(fb);
    for (int32_t i = 0; i <= order; i++) {
        int32_t val;
        if (i == 0) {
            val = 0;
        } else if (i == 1) {
            val = 1;
        } else {
            val = fb->sequence.data[i - 1] + fb->sequence.data[i - 2];
        }
        fb->sequence.data[i] = val;
        fb->sequence.size = (uint32_t)(i + 1);

        size_t fb_len = 0;
        if (example_interfaces_action_fibonacci_feedback_serialize(
                fb, app.fb_buf, sizeof(app.fb_buf), &fb_len) == 0) {
            ret = nros_action_publish_feedback(server, goal, app.fb_buf, fb_len);
            if (ret != NROS_RET_OK) {
                emit_num(NROS_LOG_SEVERITY_ERROR, "Failed to publish feedback: ", ret, __FILE__,
                         __LINE__);
            } else {
                emit(NROS_LOG_SEVERITY_INFO, "Publish feedback", __FILE__, __LINE__);
            }
        }
    }

    example_interfaces_action_fibonacci_result* result = &app.result;
    example_interfaces_action_fibonacci_result_init(result);
    result->sequence.size = fb->sequence.size;
    memcpy(result->sequence.data, fb->sequence.data, fb->sequence.size * sizeof(int32_t));

    size_t result_len = 0;
    if (example_interfaces_action_fibonacci_result_serialize(
            result, app.result_buf, sizeof(app.result_buf), &result_len) == 0) {
        ret = nros_action_succeed(server, goal, app.result_buf, result_len);
        if (ret != NROS_RET_OK) {
            emit_num(NROS_LOG_SEVERITY_ERROR, "Failed to send result: ", ret, __FILE__, __LINE__);
        } else {
            emit(NROS_LOG_SEVERITY_INFO, "Goal succeeded", __FILE__, __LINE__);
        }
    }
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
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_server", "/", &app.support),
                   1);
    g_logger = nros_node_get_logger(&app.node);

    NROS_CHECK_RET(nros_action_server_init(&app.action_server, &app.node, "/fibonacci",
                                           &fibonacci_type, goal_callback, cancel_callback,
                                           accepted_callback, &app.ctx),
                   1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 8), 1);
    NROS_CHECK_RET(nros_executor_add_action_server(&app.executor, &app.action_server), 1);

    // Spin forever: `app_main` returning is the end of the image.
    nros_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK) {
        emit(NROS_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    nros_action_server_fini(&app.action_server);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER()
