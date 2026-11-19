/* phase-573 W5d -- the SIZED C subscription registrations refuse a `msg`
 * buffer smaller than the subscription's message struct.
 *
 *     rclc_executor_add_subscription_sized(&exec, &sub, &msg, sizeof msg, cb, ON_NEW_DATA);
 *     rclc_executor_add_subscription_with_context_sized(&exec, &sub, &msg, sizeof msg,
 *                                                       cb, ctx, ON_NEW_DATA);
 *
 * rclc's own forms take `msg` with no size, so every dispatch writes the whole
 * struct into whatever the caller passed. The descriptor now states the
 * struct's size (`message_size`, the generated `sizeof`), and the sized forms
 * check against it BEFORE registering. Asserted here, over the stub RMW
 * backend:
 *   - an undersized buffer is refused by both forms, and nothing registers;
 *   - an exact one registers, and a staged sample reaches the callback,
 *     deserialised into the caller's storage;
 *   - a descriptor that states no size (`message_size == 0`) refuses nothing.
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
    int32_t pad[3]; /* larger than any undersized buffer below */
} int32_msg_t;

/* CDR: 4-byte encapsulation header (CDR_LE), then the int32. */
static int32_t int32_deserialize(const uint8_t* buffer, size_t len, void* msg) {
    if (len != 8) return -1;
    int32_t v;
    memcpy(&v, buffer + 4, sizeof v);
    memset(msg, 0, sizeof(int32_msg_t));
    ((int32_msg_t*)msg)->data = v;
    return 0;
}

static const rclc_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 8,
    .serialize = NULL,
    .deserialize = int32_deserialize,
    .message_size = sizeof(int32_msg_t),
};

/* The same type, from a descriptor that states no size. */
static const rclc_message_type_t MSG_TYPE_UNSIZED = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 8,
    .serialize = NULL,
    .deserialize = int32_deserialize,
};

static int32_msg_t s_msg_a;
static int32_msg_t s_msg_b;
static int32_msg_t s_msg_c;
static int s_calls_a = 0;
static int s_calls_b = 0;
static int32_t s_seen_a = 0;
static int32_t s_seen_b = 0;

static void on_a(const void* msgin) {
    s_calls_a++;
    s_seen_a = ((const int32_msg_t*)msgin)->data;
}

static void on_b(const void* msgin, void* ctx) {
    s_calls_b++;
    s_seen_b = ((const int32_msg_t*)msgin)->data;
    CHECK(ctx == &s_calls_b, "the context reaches the callback");
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
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 52, "sized_subscription",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "sized", &opts), RCL_RET_OK,
              "node bound to the executor");

    rcl_subscription_t sub_a = rcl_get_zero_initialized_subscription();
    rcl_subscription_t sub_b = rcl_get_zero_initialized_subscription();
    rcl_subscription_t sub_c = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&sub_a, &node, &MSG_TYPE, "/sized_a"), RCL_RET_OK,
              "subscription a created");
    CHECK_RET(rclc_subscription_init_default(&sub_b, &node, &MSG_TYPE, "/sized_b"), RCL_RET_OK,
              "subscription b created");
    CHECK_RET(rclc_subscription_init_default(&sub_c, &node, &MSG_TYPE_UNSIZED, "/sized_c"),
              RCL_RET_OK, "subscription c created");

    /* The signatures, taken as pointers so a drift fails to compile. */
    rcl_ret_t (*add_sized)(rclc_executor_t*, rcl_subscription_t*, void*, size_t,
                           rclc_subscription_callback_t, rclc_executor_handle_invocation_t) =
        rclc_executor_add_subscription_sized;
    rcl_ret_t (*add_ctx_sized)(rclc_executor_t*, rcl_subscription_t*, void*, size_t,
                               rclc_subscription_callback_with_context_t, void*,
                               rclc_executor_handle_invocation_t) =
        rclc_executor_add_subscription_with_context_sized;

    /* Undersized: refused by both forms, and nothing registers. */
    CHECK_RET(add_sized(&executor, &sub_a, &s_msg_a, sizeof(int32_t), on_a, ON_NEW_DATA),
              RCL_RET_INVALID_ARGUMENT, "an undersized buffer is refused");
    CHECK_RET(add_ctx_sized(&executor, &sub_b, &s_msg_b, sizeof(s_msg_b) - 1, on_b, &s_calls_b,
                            ON_NEW_DATA),
              RCL_RET_INVALID_ARGUMENT, "one byte short is refused (with context)");
    CHECK(rclc_executor_get_handle_count(&executor) == 0, "a refused buffer registers nothing");

    /* Exact: registered, and a sample is delivered into the caller's storage. */
    CHECK_RET(add_sized(&executor, &sub_a, &s_msg_a, sizeof s_msg_a, on_a, ON_NEW_DATA), RCL_RET_OK,
              "an exact buffer registers");
    CHECK_RET(
        add_ctx_sized(&executor, &sub_b, &s_msg_b, sizeof s_msg_b, on_b, &s_calls_b, ON_NEW_DATA),
        RCL_RET_OK, "an exact buffer registers (with context)");

    /* No size stated: no buffer is refused. */
    CHECK_RET(add_sized(&executor, &sub_c, &s_msg_c, 0, on_a, ON_NEW_DATA), RCL_RET_OK,
              "a descriptor with no size refuses nothing");
    if (s_failures != 0) return 1;

    stage_int32("/sized_a", 42);
    stage_int32("/sized_b", -7);
    for (int i = 0; i < 4; i++)
        (void)rclc_executor_spin_some(&executor, 0);

    CHECK(s_calls_a == 1 && s_seen_a == 42, "a's callback saw its sample");
    CHECK(s_calls_b == 1 && s_seen_b == -7, "b's callback saw its sample");

    if (s_failures != 0) {
        fprintf(stderr, "sized_subscription: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("sized_subscription: OK\n");
    return 0;
}
