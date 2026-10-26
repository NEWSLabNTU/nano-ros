/*
 * phase-515 C2 -- rclc's context-LESS `rclc_executor_add_subscription`,
 * called with rclc's signature on a real executor over the stub RMW backend.
 *
 *     rclc_executor_add_subscription(&exec, &sub, &msg, &cb, ON_NEW_DATA);
 *     typedef void (*rclc_subscription_callback_t)(const void *msgin);
 *
 * The registration keeps the user's callback in the arena entry's `context`
 * slot and dispatches through a fixed trampoline (RFC-0019/0020: no state in
 * the wrapper). Asserted here: one staged sample reaches the callback once,
 * deserialised into the caller's `msg`, with `msgin == &msg`; a second
 * subscription with a different callback reaches its OWN callback (the slot is
 * per entry, not shared); a NULL callback is refused.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

static int s_failures = 0;

#define CHECK(cond, msg)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL: %s (%s:%d)\n", (msg), __FILE__, __LINE__);                      \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

#define CHECK_RET(expr, expected, msg)                                                             \
    do {                                                                                           \
        rcl_ret_t _r = (expr);                                                                     \
        if (_r != (expected)) {                                                                    \
            fprintf(stderr, "FAIL: %s -- got %d, expected %d (%s:%d)\n", (msg), (int)_r,           \
                    (int)(expected), __FILE__, __LINE__);                                          \
            s_failures++;                                                                          \
        }                                                                                          \
    } while (0)

typedef struct {
    int32_t data;
} int32_msg_t;

/* CDR: 4-byte encapsulation header (CDR_LE), then the int32. */
static int32_t int32_deserialize(const uint8_t* buffer, size_t len, void* msg) {
    if (len != 8) return -1;
    int32_t v;
    memcpy(&v, buffer + 4, sizeof v);
    ((int32_msg_t*)msg)->data = v;
    return 0;
}

static const rclc_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 8,
    .serialize = NULL,
    .deserialize = int32_deserialize,
};

static int32_msg_t s_msg_a;
static int32_msg_t s_msg_b;
static int s_calls_a = 0;
static int s_calls_b = 0;
static int32_t s_seen_a = 0;
static int32_t s_seen_b = 0;
static const void* s_msgin_a = NULL;

static void on_a(const void* msgin) {
    s_calls_a++;
    s_msgin_a = msgin;
    s_seen_a = ((const int32_msg_t*)msgin)->data;
}

static void on_b(const void* msgin) {
    s_calls_b++;
    s_seen_b = ((const int32_msg_t*)msgin)->data;
}

static void stage_int32(const char* topic, int32_t value) {
    uint8_t frame[8] = {0x00, 0x01, 0x00, 0x00};
    memcpy(frame + 4, &value, sizeof value);
    CHECK(nros_stub_rmw_stage_sample(topic, frame, sizeof frame), "sample staged");
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 51, "context_less_subscription",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "ctxless", &opts), RCL_RET_OK,
              "node bound to the executor");

    rcl_subscription_t sub_a = rcl_get_zero_initialized_subscription();
    rcl_subscription_t sub_b = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&sub_a, &node, &MSG_TYPE, "/ctxless_a"), RCL_RET_OK,
              "subscription a created");
    CHECK_RET(rclc_subscription_init_default(&sub_b, &node, &MSG_TYPE, "/ctxless_b"), RCL_RET_OK,
              "subscription b created");

    /* rclc's signature, taken as a pointer so a drift fails to compile. */
    rcl_ret_t (*add)(rclc_executor_t*, rcl_subscription_t*, void*, rclc_subscription_callback_t,
                     rclc_executor_handle_invocation_t) = rclc_executor_add_subscription;

    CHECK_RET(add(&executor, &sub_a, &s_msg_a, NULL, ON_NEW_DATA), RCL_RET_INVALID_ARGUMENT,
              "a NULL callback is refused");
    CHECK_RET(add(&executor, &sub_a, &s_msg_a, on_a, ON_NEW_DATA), RCL_RET_OK,
              "context-less subscription a registered");
    CHECK_RET(add(&executor, &sub_b, &s_msg_b, on_b, ON_NEW_DATA), RCL_RET_OK,
              "context-less subscription b registered");
    if (s_failures != 0) return 1;

    stage_int32("/ctxless_a", 42);
    stage_int32("/ctxless_b", -7);
    for (int i = 0; i < 4; i++)
        (void)rclc_executor_spin_some(&executor, 0);

    CHECK(s_calls_a == 1, "subscription a's callback ran exactly once");
    CHECK(s_seen_a == 42, "a's callback saw the deserialised value");
    CHECK(s_msgin_a == &s_msg_a, "msgin is the caller's own storage");
    CHECK(s_calls_b == 1, "subscription b's callback ran exactly once");
    CHECK(s_seen_b == -7, "b's callback saw its own sample, not a's");

    if (s_failures != 0) {
        fprintf(stderr, "context_less_subscription: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("context_less_subscription: OK\n");
    return 0;
}
