/* issue 1376 — a subscription's receive slot is sized from its TYPE's bound,
 * whichever rclc entry point registers it.
 *
 * `rclc_subscription_init_*` keeps the type descriptor on the
 * `rcl_subscription_t`, and the descriptor states `serialized_size_max`. Until
 * 1376 only `rclc_executor_add_subscription_with_context` forwarded it; the
 * raw, info and group entry points passed a hint of 0, so their arena slot was
 * the executor's closure buffer however small the message. The sizing
 * descriptor credits a C entry with the typed hint and cannot see which entry
 * point a call site picks, so that was an under-size it could not report.
 *
 * Measured here with the arena's own counter, per entry point: one
 * registration of a BOUNDED type (64 bytes) against one of an UNBOUNDED type
 * (0 = no opinion, so the fallback buffer). The bounded one must claim fewer
 * bytes. Before the fix the two deltas were EQUAL for all three entry points,
 * which is what makes the comparison a test rather than a printout.
 *
 * `_raw_validated` takes the same helper; it needs the `safety-e2e` capability,
 * which this lane's library does not carry, so it is covered by the C safety
 * workspace e2e (`workspace_features::c_safety`) and the source gate
 * `check-c-subscription-bound-supplied`.
 *
 * Same stub backend as the other run probes in `just check c`.
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

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

#define MAX_HANDLES 4

static const rclc_message_type_t BOUNDED = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 64,
};

static const rclc_message_type_t UNBOUNDED = {
    .type_name = "std_msgs::msg::dds_::String_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 0,
};

static void on_message(const uint8_t* data, size_t len, void* ctx) {
    (void)data;
    (void)len;
    (void)ctx;
}

static void on_message_info(const uint8_t* data, size_t len, const uint8_t* attachment,
                            size_t attachment_len, void* ctx) {
    (void)data;
    (void)len;
    (void)attachment;
    (void)attachment_len;
    (void)ctx;
}

typedef rcl_ret_t (*add_fn)(struct rclc_executor_t*, struct rcl_subscription_t*);

static rcl_ret_t add_raw(struct rclc_executor_t* exec, struct rcl_subscription_t* sub) {
    return rclc_executor_add_subscription_raw(exec, sub, on_message, NULL, ON_NEW_DATA);
}
static rcl_ret_t add_with_info(struct rclc_executor_t* exec, struct rcl_subscription_t* sub) {
    return rclc_executor_add_subscription_with_info(exec, sub, on_message_info, NULL, ON_NEW_DATA);
}
static rcl_ret_t add_in_group(struct rclc_executor_t* exec, struct rcl_subscription_t* sub) {
    return rclc_executor_add_subscription_in_group(exec, sub, on_message, NULL, ON_NEW_DATA,
                                                   "rx_bound_group");
}

/* Bytes one registration of `type` through `add` claims from a fresh arena. */
static size_t slot_bytes(struct rclc_support_t* support, const rclc_message_type_t* type,
                         add_fn add, const char* what) {
    struct rclc_executor_t exec = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&exec, &support->context, MAX_HANDLES, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    struct rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&exec, &node, "rx_bound", &opts), RCL_RET_OK,
              "node bound to the executor");

    const size_t before = rclc_executor_get_arena_used(&exec);
    struct rcl_subscription_t sub = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&sub, &node, type, "/rx_bound"), RCL_RET_OK,
              "subscription initialised");
    CHECK_RET(add(&exec, &sub), RCL_RET_OK, what);
    const size_t after = rclc_executor_get_arena_used(&exec);

    (void)rclc_executor_remove_subscription(&exec, &sub);
    (void)rcl_subscription_fini(&sub, &node);
    (void)rcl_node_fini(&node);
    CHECK_RET(rclc_executor_fini(&exec), RCL_RET_OK, "executor finalised");
    return after - before;
}

static void compare(struct rclc_support_t* support, add_fn add, const char* name) {
    const size_t bounded = slot_bytes(support, &BOUNDED, add, name);
    const size_t unbounded = slot_bytes(support, &UNBOUNDED, add, name);
    printf("  %-44s bounded(64) %5zu B   unbounded %5zu B\n", name, bounded, unbounded);
    CHECK(bounded > 0, "a registration claims arena bytes");
    CHECK(bounded < unbounded,
          "a type that states its bound gets a slot sized from it, not the fallback buffer");
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 47, "subscription_rx_bound",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) {
        return 1;
    }

    compare(&support, add_raw, "rclc_executor_add_subscription_raw");
    compare(&support, add_with_info, "rclc_executor_add_subscription_with_info");
    compare(&support, add_in_group, "rclc_executor_add_subscription_in_group");

    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "subscription_rx_bound: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("subscription_rx_bound: OK\n");
    return 0;
}
