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

// rclc's model: each goal takes one of GOAL_HANDLES handles, its request
// deserialised into the matching `goal_requests` element; the goal callback
// only decides, and the main loop runs accepted goals between spins.
#define GOAL_HANDLES 2

typedef struct {
    uint32_t goal_count;
    rclc_action_goal_handle_t* pending[GOAL_HANDLES];
    size_t pending_count;
} server_context_t;

// All nros state — and the feedback/result staging, which is the largest thing
// this image holds per goal — in .bss, never on the reset stack.
static struct {
    rclc_support_t support;
    rcl_node_t node;
    rclc_action_server_t action_server;
    rclc_executor_t executor;
    server_context_t ctx;
    example_interfaces__action__Fibonacci_SendGoal_Request goal_requests[GOAL_HANDLES];
    example_interfaces__action__Fibonacci_FeedbackMessage feedback;
    example_interfaces__action__Fibonacci_GetResult_Response response;
    // The bytes behind the feedback and result sequences (phase-492 W7).
    example_interfaces__action__Fibonacci_Feedback__Storage fb_storage;
    example_interfaces__action__Fibonacci_Result__Storage result_storage;
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

static rcl_ret_t goal_callback(rclc_action_goal_handle_t* goal_handle, void* context) {
    server_context_t* ctx = (server_context_t*)context;
    const example_interfaces__action__Fibonacci_SendGoal_Request* req =
        (const example_interfaces__action__Fibonacci_SendGoal_Request*)
            goal_handle->ros_goal_request;
    emit_num(RCLC_LOG_SEVERITY_INFO, "Received goal request with order ", req->goal.order, __FILE__,
             __LINE__);
    // The sequence holds 64 terms; anything that would overrun it is refused.
    if (req->goal.order < 0 || req->goal.order >= 64) {
        emit(RCLC_LOG_SEVERITY_WARN, "Goal rejected: order out of range", __FILE__, __LINE__);
        return RCL_RET_ACTION_GOAL_REJECTED;
    }
    if (ctx->pending_count >= GOAL_HANDLES) {
        return RCL_RET_ACTION_GOAL_REJECTED;
    }
    ctx->pending[ctx->pending_count++] = goal_handle;
    return RCL_RET_ACTION_GOAL_ACCEPTED;
}

static bool cancel_callback(rclc_action_goal_handle_t* goal_handle, void* context) {
    (void)goal_handle;
    (void)context;
    emit(RCLC_LOG_SEVERITY_INFO, "Cancel request for goal", __FILE__, __LINE__);
    return true;
}

static void execute_goal(server_context_t* ctx, rclc_action_goal_handle_t* goal_handle) {
    const example_interfaces__action__Fibonacci_SendGoal_Request* req =
        (const example_interfaces__action__Fibonacci_SendGoal_Request*)
            goal_handle->ros_goal_request;
    int32_t order = req->goal.order;
    ctx->goal_count++;
    emit(RCLC_LOG_SEVERITY_INFO, "Executing goal", __FILE__, __LINE__);

    example_interfaces__action__Fibonacci_Feedback* fb = &app.feedback.feedback;
    example_interfaces__action__Fibonacci_Feedback__init_in(fb, &app.fb_storage);
    rcl_ret_t ret;
    for (int32_t i = 0; i <= order && !goal_handle->goal_cancelled; i++) {
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

        // rclc_action_publish_feedback writes the goal id into the message.
        ret = rclc_action_publish_feedback(goal_handle, &app.feedback);
        if (ret != RCL_RET_OK) {
            emit_num(RCLC_LOG_SEVERITY_ERROR, "Failed to publish feedback: ", ret, __FILE__,
                     __LINE__);
        } else {
            emit(RCLC_LOG_SEVERITY_INFO, "Publish feedback", __FILE__, __LINE__);
        }
    }

    example_interfaces__action__Fibonacci_Result* result = &app.response.result;
    example_interfaces__action__Fibonacci_Result__init_in(result, &app.result_storage);
    result->sequence.size = fb->sequence.size;
    memcpy(result->sequence.data, fb->sequence.data, fb->sequence.size * sizeof(int32_t));

    rcl_action_goal_state_t status =
        goal_handle->goal_cancelled ? GOAL_STATE_CANCELED : GOAL_STATE_SUCCEEDED;
    ret = rclc_action_send_result(goal_handle, status, &app.response);
    if (ret != RCL_RET_OK) {
        emit_num(RCLC_LOG_SEVERITY_ERROR, "Failed to send result: ", ret, __FILE__, __LINE__);
    } else if (status == GOAL_STATE_SUCCEEDED) {
        emit(RCLC_LOG_SEVERITY_INFO, "Goal succeeded", __FILE__, __LINE__);
    } else {
        emit(RCLC_LOG_SEVERITY_INFO, "Goal canceled", __FILE__, __LINE__);
    }
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // `rclc_support_init` takes the connect configuration from the build
    // (`<rclc/detail/entry_config.h>`, issue 0946); on this leaf that is the empty
    // bottom rung, and baking `system.toml`'s locator is issue 1512's.
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "fibonacci_action_server", "/", &app.support),
                   1);
    g_logger = rclc_node_get_logger(&app.node);

    RCLC_CHECK_RET(rclc_action_server_init_default(
                       &app.action_server, &app.node, &app.support,
                       ROSIDL_GET_ACTION_TYPE_SUPPORT(example_interfaces, action, Fibonacci),
                       "/fibonacci"),
                   1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 8, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_add_action_server(&app.executor, &app.action_server, GOAL_HANDLES,
                                                   app.goal_requests, sizeof(app.goal_requests[0]),
                                                   goal_callback, cancel_callback, &app.ctx),
                   1);

    // The readiness marker the harness waits on (`expect_ready`, phase-342 W7).
    emit(RCLC_LOG_SEVERITY_INFO, "Waiting for action goals", __FILE__, __LINE__);

    // Spin forever, running whatever goals each spin accepted:
    // `app_main` returning is the end of the image.
    for (;;) {
        rcl_ret_t ret = rclc_executor_spin_some(&app.executor, 100000000ULL);
        if (ret != RCL_RET_OK && ret != RCL_RET_TIMEOUT) {
            emit(RCLC_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
            break;
        }
        for (size_t i = 0; i < app.ctx.pending_count; i++) {
            execute_goal(&app.ctx, app.ctx.pending[i]);
        }
        app.ctx.pending_count = 0;
    }

    rclc_executor_fini(&app.executor);
    rclc_action_server_fini(&app.action_server, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}
