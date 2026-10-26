/* Issue 2060 — a pushed subscription's status events are polled with no
 * arrival.
 *
 * Every backend that carries subscription status events polls them from the
 * subscription's `has_data` (zenoh: deadline, liveliness, matched; Cyclone:
 * the `take_event` drain through the cffi adapter). Since phase-397 W3 the
 * executor reads an ATTRIBUTED subscription's readiness from its arrival bit
 * and asks `has_data` only after an arrival — so once the last sample was
 * taken nothing asked again, and the events about NOTHING arriving (the last
 * publisher left, a deadline passed) were never observed. Measured live by
 * issue 2039's interop cell: the matched count never fell.
 *
 * The stub backend accepts entity wakers (so its subscription is attributed),
 * counts `has_data` calls, and can fire a staged count event from inside the
 * next `has_data` — where the real backends fire it. This probe checks:
 *
 *   1. CONTROL — with no status callback, idle passes ask `has_data` NOTHING:
 *      the phase-397 property, which the fix must not cost;
 *   2. once a status callback is registered, every pass asks, and an event
 *      staged with NO arrival reaches the user callback on the next pass;
 *   3. removing the subscription forgets that: a re-added subscription with
 *      no callback is pushed and unprobed again.
 *
 * Mutation-checked: dropping the `entity_status_polled` fold from the
 * readiness scan (`spin.rs`) fails (2); not clearing it on release fails (3).
 */

#include "stub_rmw_backend.h"

#include <rclc/rclc.h>

#include <stdbool.h>
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

static const rclc_message_type_t MSG_TYPE = {
    .type_name = "std_msgs::msg::dds_::Int32_",
    .type_hash = "RIHS01_0000000000000000000000000000000000000000000000000000000000000000",
    .serialized_size_max = 64,
};

/* `rmw_event_type_t` value (`<nros/rmw_event.h>`). */
enum { K_REQUESTED_DEADLINE_MISSED = 1 };

/* Idle passes measured per phase. */
enum { PASSES = 5 };

static int s_deadline_calls = 0;
static uint32_t s_deadline_total = 0;

static void on_message(const uint8_t* data, size_t len, void* ctx) {
    (void)data;
    (void)len;
    (void)ctx;
}

static void on_deadline(struct rcl_subscription_t* sub, struct rclc_count_status_t status,
                        void* ctx) {
    (void)sub;
    (void)ctx;
    s_deadline_calls++;
    s_deadline_total = status.total_count;
}

/* Spin `n` zero-timeout passes and return how many `has_data` calls they made. */
static uint32_t probes_over(rclc_executor_t* executor, int n) {
    const uint32_t from = nros_stub_rmw_subscription_has_data_calls();
    for (int i = 0; i < n; i++) (void)rclc_executor_spin_some(executor, 0);
    return nros_stub_rmw_subscription_has_data_calls() - from;
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);
    nros_stub_rmw_set_entity_wake_supported(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 49, "subscription_status_poll",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "status_poll", &opts), RCL_RET_OK,
              "node bound to the executor");
    rcl_subscription_t sub = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&sub, &node, &MSG_TYPE, "/status_poll"), RCL_RET_OK,
              "subscription created");
    CHECK_RET(rclc_executor_add_subscription_raw(&executor, &sub, on_message, NULL, ON_NEW_DATA),
              RCL_RET_OK, "subscription added to the executor");
    if (s_failures != 0) return 1;

    /* The first pass after arming asks once (data may predate the waker). */
    (void)probes_over(&executor, 2);

    /* ---- 1. control: no status callback, no probe ---- */
    {
        const uint32_t probes = probes_over(&executor, PASSES);
        if (probes != 0) {
            fprintf(stderr,
                    "FAIL: %d idle passes over an attributed subscription with no status "
                    "callback asked has_data %u time(s); a pushed slot is read from its "
                    "arrival bit (phase-397 W3)\n",
                    PASSES, (unsigned)probes);
            s_failures++;
        }
    }

    /* ---- 2. a status callback makes every pass ask ---- */
    CHECK_RET(rclc_subscription_set_requested_deadline_missed_callback(&sub, 100, on_deadline,
                                                                       NULL),
              RCL_RET_OK, "deadline callback registered");
    nros_stub_rmw_stage_polled_count_event(K_REQUESTED_DEADLINE_MISSED, 3, 1);
    CHECK(s_deadline_calls == 0, "a polled event does not fire before any has_data runs");
    (void)rclc_executor_spin_some(&executor, 0);
    if (s_deadline_calls != 1 || s_deadline_total != 3) {
        fprintf(stderr,
                "FAIL: a deadline event polled from has_data, with NO arrival, reached the user "
                "callback %d time(s) (total=%u) on the next pass; expected once with total=3 "
                "(issue 2060)\n",
                s_deadline_calls, (unsigned)s_deadline_total);
        s_failures++;
    }
    {
        const uint32_t probes = probes_over(&executor, PASSES);
        if (probes < PASSES) {
            fprintf(stderr,
                    "FAIL: %d idle passes over a subscription WITH a status callback asked "
                    "has_data %u time(s); every pass must (issue 2060)\n",
                    PASSES, (unsigned)probes);
            s_failures++;
        }
    }

    /* ---- 3. removal forgets the mark ---- */
    CHECK_RET(rclc_executor_remove_subscription(&executor, &sub), RCL_RET_OK,
              "subscription removed from the executor");
    CHECK_RET(rclc_executor_add_subscription_raw(&executor, &sub, on_message, NULL, ON_NEW_DATA),
              RCL_RET_OK, "subscription re-added, with no status callback");
    (void)probes_over(&executor, 2);
    {
        const uint32_t probes = probes_over(&executor, PASSES);
        if (probes != 0) {
            fprintf(stderr,
                    "FAIL: after remove + re-add with no status callback, %d idle passes asked "
                    "has_data %u time(s); the mark must leave with its entry\n",
                    PASSES, (unsigned)probes);
            s_failures++;
        }
    }

    (void)rcl_subscription_fini(&sub, &node);
    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "subscription_status_poll: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("subscription_status_poll: OK (no probe without a status callback; with one every "
           "pass asks and a no-arrival event is delivered; removal clears it)\n");
    return 0;
}
