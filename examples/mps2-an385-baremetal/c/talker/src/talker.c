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
///     `NROS_LOG_INFO(logger, "…%d…")` macros would print their own format
///     string here. Records are built by hand and emitted through
///     `nros_log_emit_at`, which takes text and takes no format.
///
/// `NROS_APP_MAIN_REGISTER()` emits `void app_main(void)` on this platform, and
/// `src/main.rs` is the startup chain that calls it after the board is up —
/// which is exactly the contract `<nros/app_main.h>` already documents for
/// bare metal.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
// `strlen` only. It is one of the 19 freestanding symbols
// `nros-baremetal-common`'s `libc-stubs` defines, which is the whole libc this
// image has; the declaration comes from the toolchain's header.
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/log.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/rcl_compat.h>
#include <nros/timer.h>

// Generated message bindings (`nros generate c`, run by build.rs).
#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Backend registration — the image's, exactly as on the cmake road
// ----------------------------------------------------------------------------
//
// `nros_support_init` calls `nros_app_register_backends()` unconditionally: the
// weak no-op was removed in phase-249 P4a, so every C/C++ image supplies a
// strong definition. On the cmake road `nano_ros_link_rmw()` GENERATES this exact
// TU into the build dir; a cargo-rooted image has no such generator, so it is
// written here.
//
// Not a Rust `nros_rmw_zenoh::register()` in `main.rs` instead, though that call
// exists and is idempotent (`nros-board-mps2-an385`'s own `entry::boot` makes it):
// this is the hook `nros_support_init` itself reaches, so defining it is what
// makes the ordering a property of the runtime rather than of the boot shim.
extern int nros_rmw_zenoh_register(void);

void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

// ----------------------------------------------------------------------------
// Application state — all in .bss, never on the reset stack
// ----------------------------------------------------------------------------

typedef struct {
    nros_publisher_t* publisher;
    std_msgs_msg_string message;
    int32_t count;
} talker_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t publisher;
    talker_context_t talker_ctx;
    nros_timer_t timer;
    nros_executor_t executor;
} app;

static nros_logger_t g_logger;

// ----------------------------------------------------------------------------
// Hand-rolled formatting — see the file header for why printf is unavailable
// ----------------------------------------------------------------------------

// Both take the buffer's CAPACITY and truncate rather than run past it. On a
// Cortex-M3 with no MPU a one-byte overrun is silent corruption of whatever
// `.bss` or stack frame follows, and a buffer here is fed a message payload
// whose bound is `sizeof(std_msgs_msg_string::data)` — 256 — so "the payload is
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
static void emit(nros_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    nros_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

// ----------------------------------------------------------------------------
// Timer callback — publish one message
// ----------------------------------------------------------------------------

static void timer_callback(struct nros_timer_t* timer, void* context) {
    (void)timer;
    talker_context_t* ctx = (talker_context_t*)context;

    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker and every sibling example in this tree.
    ctx->count++;
    const size_t msg_cap = sizeof(ctx->message.data);
    size_t pos = append_str(ctx->message.data, msg_cap, 0u, "Hello World: ");
    pos = append_u32(ctx->message.data, msg_cap, pos, (uint32_t)ctx->count);
    ctx->message.data[pos] = '\0';

    NROS_SOFTCHECK(std_msgs_msg_string_publish(ctx->publisher, &ctx->message));

    // The marker the QEMU e2e greps for, spelled the way the `rust/talker`
    // sibling spells it (`nros_tests::output` owns the constant). Sized from the
    // payload's own bound plus the decoration, not from what the payload happens
    // to be today.
    char line[sizeof(ctx->message.data) + 16u];
    size_t len = append_str(line, sizeof(line), 0u, "Publishing: '");
    len = append_str(line, sizeof(line), len, ctx->message.data);
    len = append_str(line, sizeof(line), len, "'");
    line[len] = '\0';
    emit(NROS_LOG_SEVERITY_INFO, line, __FILE__, __LINE__);
}

// ----------------------------------------------------------------------------
// Entry
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    // `NROS_ENTRY_LOCATOR` is the ONE producer of the compile-time connect
    // configuration (`<nros/entry_config.h>`, issue 0946). On this leaf it is
    // the empty bottom rung, which the linked backend substitutes its own
    // default for; baking this image's `[image.*] locator` from `system.toml`
    // is what the runtime lane needs and is tracked on issue 1512.
    NROS_CHECK_RET(
        nros_support_init(&app.support, NROS_ENTRY_LOCATOR, (uint8_t)NROS_ENTRY_DOMAIN_ID), 1);
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "talker", "/", &app.support), 1);
    g_logger = nros_node_get_logger(&app.node);

    NROS_CHECK_RET(rclc_publisher_init_default(&app.publisher, &app.node,
                                               std_msgs_msg_string_get_type_support(), "/chatter"),
                   1);

    app.talker_ctx.publisher = &app.publisher;
    app.talker_ctx.count = 0;
    std_msgs_msg_string_init(&app.talker_ctx.message);

    NROS_CHECK_RET(
        nros_timer_init(&app.timer, &app.support, 1000000000ULL, timer_callback, &app.talker_ctx),
        1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(rclc_executor_add_timer(&app.executor, &app.timer), 1);

    // Spin forever with a 100 ms period. There is no signal to cancel on and no
    // process to return to: on this board `app_main` returning is the end of the
    // image, which `src/main.rs` turns into a semihosting exit.
    nros_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK) {
        emit(NROS_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    rcl_timer_fini(&app.timer);
    nros_publisher_fini(&app.publisher);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER()
