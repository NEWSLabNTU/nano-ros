/// @file action_server.c
/// @brief C Fibonacci action server for the PURE BARE-METAL MPS2-AN385.
///
/// The same program as `examples/mps2-an385-freertos/c/action-server`, minus
/// what a board with no RTOS does not have (the `c/talker` sibling's file header
/// has the long form, issue 1512): no libc and no `printf` substitution, so
/// records are built by hand and emitted through `rclc_log_emit_at`.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include <rclc/action_server.h>
#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rcl/rcl.h>

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
    rclc_support_t support;
    rcl_node_t node;
    rcl_action_server_t action_server;
    rclc_executor_t executor;
    server_context_t ctx;
    example_interfaces__action__Fibonacci_Feedback fb;
    example_interfaces__action__Fibonacci_Result result;
    // The bytes behind `fb.sequence` / `result.sequence` (phase-492 W7).
    example_interfaces__action__Fibonacci_Feedback__Storage fb_storage;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
    uint8_t fb_buf[512];
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

static void emit(rcl_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    rclc_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

/// One line of fixed text followed by a number.
static void emit_num(rcl_log_severity_t severity, const char* text, int64_t value, const char* file,
                     uint32_t line) {
    char buf[96];
    size_t len = append_str(buf, sizeof(buf), 0u, text);
    len = append_i64(buf, sizeof(buf), len, value);
    buf[len] = '\0';
    emit(severity, buf, file, line);
}

static rclc_goal_response_t goal_callback(rcl_action_server_t* server,
                                          const rclc_goal_handle_t* goal,
                                          const uint8_t* goal_request, size_t goal_len,
                                          void* context) {
    (void)server;
    (void)goal;
    example_interfaces__action__Fibonacci_Goal goal_msg;
    if (example_interfaces__action__Fibonacci_Goal__deserialize(&goal_msg, goal_request,
                                                                goal_len) != 0) {
        emit(RCLC_LOG_SEVERITY_ERROR, "Failed to deserialize goal", __FILE__, __LINE__);
        return RCLC_GOAL_REJECT;
    }
    emit_num(RCLC_LOG_SEVERITY_INFO, "Received goal request with order ", goal_msg.order, __FILE__,
             __LINE__);
    // The sequence holds 64 terms; anything that would overrun it is refused.
    if (goal_msg.order < 0 || goal_msg.order >= 64) {
        emit(RCLC_LOG_SEVERITY_WARN, "Goal rejected: order out of range", __FILE__, __LINE__);
        return RCLC_GOAL_REJECT;
    }
    server_context_t* ctx = (server_context_t*)context;
    if (ctx != NULL) {
        ctx->accepted_order = goal_msg.order;
    }
    return RCLC_GOAL_ACCEPT_AND_EXECUTE;
}

static rclc_cancel_response_t cancel_callback(rcl_action_server_t* server,
                                              const rclc_goal_handle_t* goal, void* context) {
    (void)server;
    (void)goal;
    (void)context;
    emit(RCLC_LOG_SEVERITY_INFO, "Cancel request for goal", __FILE__, __LINE__);
    return RCLC_CANCEL_ACCEPT;
}

static void accepted_callback(rcl_action_server_t* server, const rclc_goal_handle_t* goal,
                              void* context) {
    server_context_t* ctx = (server_context_t*)context;
    ctx->goal_count++;
    emit(RCLC_LOG_SEVERITY_INFO, "Executing goal", __FILE__, __LINE__);

    int32_t order = ctx->accepted_order;
    rcl_ret_t ret = rclc_action_execute(server, goal);
    if (ret != RCL_RET_OK) {
        emit_num(RCLC_LOG_SEVERITY_ERROR, "Failed to set executing state: ", ret, __FILE__,
                 __LINE__);
        return;
    }

    example_interfaces__action__Fibonacci_Feedback* fb = &app.fb;
    example_interfaces__action__Fibonacci_Feedback__init_in(fb, &app.fb_storage);
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
        if (example_interfaces__action__Fibonacci_Feedback__serialize(
                fb, app.fb_buf, sizeof(app.fb_buf), &fb_len) == 0) {
            ret = rclc_action_publish_feedback_raw(server, goal, app.fb_buf, fb_len);
            if (ret != RCL_RET_OK) {
                emit_num(RCLC_LOG_SEVERITY_ERROR, "Failed to publish feedback: ", ret, __FILE__,
                         __LINE__);
            } else {
                emit(RCLC_LOG_SEVERITY_INFO, "Publish feedback", __FILE__, __LINE__);
            }
        }
    }

    example_interfaces__action__Fibonacci_Result* result = &app.result;
    example_interfaces__action__Fibonacci_Result__init_in(result, &app.result_storage);
    result->sequence.size = fb->sequence.size;
    memcpy(result->sequence.data, fb->sequence.data, fb->sequence.size * sizeof(int32_t));

    size_t result_len = 0;
    if (example_interfaces__action__Fibonacci_Result__serialize(
            result, app.result_buf, sizeof(app.result_buf), &result_len) == 0) {
        ret = rclc_action_succeed(server, goal, app.result_buf, result_len);
        if (ret != RCL_RET_OK) {
            emit_num(RCLC_LOG_SEVERITY_ERROR, "Failed to send result: ", ret, __FILE__, __LINE__);
        } else {
            emit(RCLC_LOG_SEVERITY_INFO, "Goal succeeded", __FILE__, __LINE__);
        }
    }
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
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_server", "/", &app.support),
                   1);
    g_logger = rclc_node_get_logger(&app.node);

    RCLC_CHECK_RET(rclc_action_server_init_with_callbacks(
                       &app.action_server, &app.node, "/fibonacci", &fibonacci_type, goal_callback,
                       cancel_callback, accepted_callback, &app.ctx),
                   1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 8, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_attach_action_server(&app.executor, &app.action_server), 1);

    // The readiness marker the harness waits on (`expect_ready`, phase-342 W7).
    emit(RCLC_LOG_SEVERITY_INFO, "Waiting for action goals", __FILE__, __LINE__);

    // Spin forever: `app_main` returning is the end of the image.
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK) {
        emit(RCLC_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    rcl_action_server_fini(&app.action_server, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER()
