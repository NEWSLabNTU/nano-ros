/// @file listener.c
/// @brief C listener for the PURE BARE-METAL MPS2-AN385 — subscribes to
///        `std_msgs/String` on `/chatter`.
///
/// The same program as `examples/mps2-an385-freertos/c/listener/src/main.c`,
/// minus the two things a board with no RTOS does not have (the `c/talker`
/// sibling's file header has the long form, issue 1512): no libc (`stdio`,
/// `getenv`, `signal`), and no `printf` substitution — so records are built by
/// hand and emitted through `nros_log_emit_at`.

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
#include <nros/subscription.h>

#include "std_msgs.h"

// Backend registration — the image's, exactly as on the cmake road (see the
// `c/talker` sibling for why it is written here rather than in `main.rs`).
extern int nros_rmw_zenoh_register(void);

void nros_app_register_backends(void) {
    (void)nros_rmw_zenoh_register();
}

typedef struct {
    uint32_t message_count;
} listener_context_t;

// All nros state in .bss, never on the reset stack.
static struct {
    nros_support_t support;
    nros_node_t node;
    listener_context_t listener_ctx;
    nros_subscription_t subscription;
    nros_executor_t executor;
    // The message the callback is handed: the executor deserializes INTO it
    // (rclc's shape), so it must outlive the subscription — trivially true here.
    std_msgs_msg_string msg;
} app;

static nros_logger_t g_logger;

/// Append the NUL-terminated `text` at `out[pos]`, returning the new position.
/// Truncates rather than run past `cap`, leaving room for the caller's NUL.
static size_t append_str(char* out, size_t cap, size_t pos, const char* text) {
    while (*text != '\0' && pos + 1u < cap) {
        out[pos++] = *text++;
    }
    return pos;
}

static void emit(nros_log_severity_t severity, const char* text, const char* file, uint32_t line) {
    nros_log_emit_at(g_logger, severity, text, strlen(text), file, line);
}

// rclc's callback shape: a DESERIALIZED message. A sample that cannot be decoded
// into `app.msg` never reaches here — the executor drops and counts it.
static void subscription_callback(const void* msgin, void* context) {
    const std_msgs_msg_string* msg = (const std_msgs_msg_string*)msgin;
    listener_context_t* ctx = (listener_context_t*)context;
    ctx->message_count++;

    // The marker the e2e greps for (`nros_tests::output` owns the constant),
    // sized from the payload's own bound plus the decoration.
    char line[sizeof(msg->data) + 16u];
    size_t len = append_str(line, sizeof(line), 0u, "I heard: [");
    len = append_str(line, sizeof(line), len, msg->data);
    len = append_str(line, sizeof(line), len, "]");
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
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "listener", "/", &app.support), 1);
    g_logger = nros_node_get_logger(&app.node);

    NROS_CHECK_RET(rclc_subscription_init_default(&app.subscription, &app.node,
                                                  std_msgs_msg_string_get_type_support(),
                                                  "/chatter"),
                   1);

    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_CHECK_RET(std_msgs_msg_string_executor_add_subscription(
                       &app.executor, &app.subscription, &app.msg, subscription_callback,
                       &app.listener_ctx, NROS_EXECUTOR_ON_NEW_DATA),
                   1);

    // The readiness marker the harness waits on (`expect_ready`, phase-342 W7).
    emit(NROS_LOG_SEVERITY_INFO, "Subscriber created for topic: /chatter", __FILE__, __LINE__);

    // Spin forever: `app_main` returning is the end of the image.
    nros_ret_t ret = rclc_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK) {
        emit(NROS_LOG_SEVERITY_ERROR, "executor spin failed", __FILE__, __LINE__);
    }

    rclc_executor_fini(&app.executor);
    nros_subscription_fini(&app.subscription);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER()
