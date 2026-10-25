/// @file talker.c
/// @brief C talker for the PURE BARE-METAL MPS2-AN385 — publishes
///        `std_msgs/String` "Hello World: N" from a 1 Hz timer.
///
/// The application is ordinary nano-ros C, identical in shape to
/// `examples/mps2-an385-freertos/c/talker/src/main.c`. Two things differ, and
/// both are properties of having no RTOS rather than of the C API (issue 1512):
///
///  1. **No libc.** There is no `stdio.h`, no `getenv`, no `signal`. The
///     freestanding string/heap helpers this file and the generated bindings
///     need (`memset`, `memcpy`, `malloc`) come from `nros-baremetal-common`.
///  2. **No `printf` substitution.** `nros-baremetal-common`'s `vsnprintf` is a
///     stub that copies the format string VERBATIM (deliberately — see its
///     doc comment: muteness is worse than an unsubstituted message), so the
///     `RCLC_LOG_INFO(logger, "…%d…")` macros would print their own format
///     string here. Records are built by hand and emitted through
///     `rclc_log_emit_at`, which takes text and takes no format.
///
/// The program writes `main`; the build compiles it as `nros_ported_main` and
/// `src/main.rs` reaches it as `nros_app_main` once the board is up (phase-499).

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
// `strlen` only. It is one of the 19 freestanding symbols
// `nros-baremetal-common`'s `libc-stubs` defines, which is the whole libc this
// image has; the declaration comes from the toolchain's header.
#include <string.h>

#include <rclc/executor.h>
#include <rclc/init.h>
#include <rclc/node.h>
#include <rclc/publisher.h>
#include <rcl/rcl.h>
#include <rclc/timer.h>

// Generated message bindings (`nros generate c`, run by build.rs).
#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Application state — all in .bss, never on the reset stack
// ----------------------------------------------------------------------------

typedef struct {
    rcl_publisher_t* publisher;
    std_msgs__msg__String message;
    // The bytes behind `message.data` (phase-492 W7), wired by
    // `std_msgs__msg__String__init_in`. The caller owns them; nothing allocates.
    std_msgs__msg__String__Storage message_storage;
    int32_t count;
} talker_context_t;

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_publisher_t publisher;
    talker_context_t talker_ctx;
    rcl_timer_t timer;
    rclc_executor_t executor;
} app;

static rclc_logger_t g_logger;

// ----------------------------------------------------------------------------
// Hand-rolled formatting — see the file header for why printf is unavailable
// ----------------------------------------------------------------------------

// Both take the buffer's CAPACITY and truncate rather than run past it. On a
// Cortex-M3 with no MPU a one-byte overrun is silent corruption of whatever
// `.bss` or stack frame follows, and a buffer here is fed a message payload
// whose bound is `sizeof(std_msgs__msg__String::data)` — 256 — so "the payload is
// short in practice" is not a bound. `cap` is the whole buffer; each function
// leaves room for the caller's NUL.

/// Append `value` in decimal at `out[pos]`, returning the new position.
static size_t append_u32(char* out, size_t cap, size_t pos, uint32_t value) {
    char digits[10];
    size_t n = 0;
    do {
        digits[n++] = (char)('0' + (value % 10u));
        value /= 10u;
    } while (value != 0u && n < sizeof(digits));
    while (n > 0u && pos + 1u < cap) {
        out[pos++] = digits[--n];
    }
    return pos;
}

/// Append the NUL-terminated `text` at `out[pos]`, returning the new position.
static size_t append_str(char* out, size_t cap, size_t pos, const char* text) {
    while (*text != '\0' && pos + 1u < cap) {
        out[pos++] = *text++;
    }
    return pos;
}

/// Emit one already-built line, so no call site hand-counts a length.
static void emit(rcl_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    rclc_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

// ----------------------------------------------------------------------------
// Timer callback — publish one message
// ----------------------------------------------------------------------------

static void timer_callback(struct rcl_timer_t* timer, int64_t last_call_time) {
    (void)timer;
    talker_context_t* ctx = &app.talker_ctx;
    (void)last_call_time;

    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker and every sibling example in this tree.
    ctx->count++;
    const size_t msg_cap = ctx->message.data.capacity;
    size_t pos = append_str(ctx->message.data.data, msg_cap, 0u, "Hello World: ");
    pos = append_u32(ctx->message.data.data, msg_cap, pos, (uint32_t)ctx->count);
    ctx->message.data.data[pos] = '\0';
    ctx->message.data.size = pos;

    RCLC_SOFTCHECK(rcl_publish(ctx->publisher, &ctx->message, NULL));

    // The marker the QEMU e2e greps for, spelled the way the `rust/talker`
    // sibling spells it (`nros_tests::output` owns the constant). Sized from the
    // payload's own bound plus the decoration, not from what the payload happens
    // to be today.
    char line[sizeof(ctx->message_storage.data) + 16u];
    size_t len = append_str(line, sizeof(line), 0u, "Publishing: '");
    len = append_str(line, sizeof(line), len, ctx->message.data.data);
    len = append_str(line, sizeof(line), len, "'");
    line[len] = '\0';
    emit(RCLC_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

// ----------------------------------------------------------------------------
// Entry
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // `rclc_support_init` takes the connect configuration from the build
    // (`<rclc/detail/entry_config.h>`, issue 0946); on this leaf that is the empty
    // bottom rung, and baking `system.toml`'s locator is issue 1512's.
    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "talker", "/", &app.support), 1);
    g_logger = rclc_node_get_logger(&app.node);

    RCLC_CHECK_RET(rclc_publisher_init_default(&app.publisher, &app.node,
                                               ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String),
                                               "/chatter"),
                   1);

    app.talker_ctx.publisher = &app.publisher;
    app.talker_ctx.count = 0;
    std_msgs__msg__String__init_in(&app.talker_ctx.message, &app.talker_ctx.message_storage);

    RCLC_CHECK_RET(rclc_timer_init_default(&app.timer, &app.support, 1000000000ULL, timer_callback),
                   1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 4, NULL), 1);
    RCLC_CHECK_RET(rclc_executor_add_timer(&app.executor, &app.timer), 1);

    // Spin forever with a 100 ms period. There is no signal to cancel on and no
    // process to return to: on this board `app_main` returning is the end of the
    // image, which `src/main.rs` turns into a semihosting exit.
    rcl_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != RCL_RET_OK) {
        emit(RCLC_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    rcl_timer_fini(&app.timer);
    rcl_publisher_fini(&app.publisher, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}
