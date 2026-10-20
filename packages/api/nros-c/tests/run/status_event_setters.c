/* Issue 1993 — the five Tier-1 status-event setters reach the backend.
 *
 * `rclc_{subscription,publisher}_set_*_callback` for liveliness, deadline and
 * message-lost returned `RCL_RET_UNSUPPORTED` unconditionally, on every
 * backend, ignoring their arguments. They now register through the same
 * `status_event_slots` table the issue-1972 matched setters use. This probe
 * drives every one of them against the stub backend:
 *
 *   1. each setter returns `RCL_RET_OK`, and the stub holds a registration
 *      for that kind carrying the `deadline_ms` the caller passed (0 for the
 *      kinds that take none);
 *   2. firing it the way a backend's event path would calls the USER callback
 *      with the right entity, the right status and the right context;
 *   3. a subscription not yet added to an executor answers `RCL_RET_NOT_INIT`;
 *   4. a backend that carries no status events (the XRCE / uORB answer)
 *      makes every setter return `RCL_RET_UNSUPPORTED`, not `RCL_RET_OK`;
 *   5. after `rcl_*_fini` the stub still holds the registration — as a real
 *      backend's listener can — and firing it is a no-op.
 *
 * Negative control for (2): before any registration the stub holds nothing,
 * so a fire returns false — a probe that passed without the wiring would be
 * reading a callback nobody installed. */

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

/* rmw_event_type_t values (`<nros/rmw_event.h>`). */
enum {
    K_LIVELINESS_CHANGED = 0,
    K_REQUESTED_DEADLINE_MISSED = 1,
    K_MESSAGE_LOST = 2,
    K_LIVELINESS_LOST = 3,
    K_OFFERED_DEADLINE_MISSED = 4,
    K_COUNT = 5,
};

/* What the last callback of each kind saw. */
typedef struct {
    int calls;
    void* entity;
    void* ctx;
    rclc_count_status_t count;
    rclc_liveliness_changed_status_t live;
} seen_t;

static seen_t s_seen[K_COUNT];
static int s_ctx_ids[K_COUNT] = {K_LIVELINESS_CHANGED, K_REQUESTED_DEADLINE_MISSED,
                                 K_MESSAGE_LOST, K_LIVELINESS_LOST, K_OFFERED_DEADLINE_MISSED};

static void on_message(const uint8_t* data, size_t len, void* ctx) {
    (void)data;
    (void)len;
    (void)ctx;
}

static void on_liveliness_changed(struct rcl_subscription_t* sub,
                                  struct rclc_liveliness_changed_status_t status, void* ctx) {
    seen_t* s = &s_seen[K_LIVELINESS_CHANGED];
    s->calls++;
    s->entity = sub;
    s->ctx = ctx;
    s->live = status;
}

static void on_sub_count(struct rcl_subscription_t* sub, struct rclc_count_status_t status,
                         void* ctx) {
    int kind = *(const int*)ctx;
    seen_t* s = &s_seen[kind];
    s->calls++;
    s->entity = sub;
    s->ctx = ctx;
    s->count = status;
}

static void on_pub_count(struct rcl_publisher_t* pub, struct rclc_count_status_t status,
                         void* ctx) {
    int kind = *(const int*)ctx;
    seen_t* s = &s_seen[kind];
    s->calls++;
    s->entity = pub;
    s->ctx = ctx;
    s->count = status;
}

static int total_calls(void) {
    int n = 0;
    for (int k = 0; k < K_COUNT; k++) n += s_seen[k].calls;
    return n;
}

/* Register all five; `expected` is what every setter must answer. */
static void register_all(struct rcl_subscription_t* sub, struct rcl_publisher_t* pub,
                         rcl_ret_t expected, const char* phase) {
    char msg[160];
    snprintf(msg, sizeof msg, "%s: liveliness changed", phase);
    CHECK_RET(rclc_subscription_set_liveliness_changed_callback(sub, on_liveliness_changed,
                                                                &s_ctx_ids[K_LIVELINESS_CHANGED]),
              expected, msg);
    snprintf(msg, sizeof msg, "%s: requested deadline missed", phase);
    CHECK_RET(rclc_subscription_set_requested_deadline_missed_callback(
                  sub, 250, on_sub_count, &s_ctx_ids[K_REQUESTED_DEADLINE_MISSED]),
              expected, msg);
    snprintf(msg, sizeof msg, "%s: message lost", phase);
    CHECK_RET(rclc_subscription_set_message_lost_callback(sub, on_sub_count,
                                                          &s_ctx_ids[K_MESSAGE_LOST]),
              expected, msg);
    snprintf(msg, sizeof msg, "%s: liveliness lost", phase);
    CHECK_RET(rclc_publisher_set_liveliness_lost_callback(pub, on_pub_count,
                                                          &s_ctx_ids[K_LIVELINESS_LOST]),
              expected, msg);
    snprintf(msg, sizeof msg, "%s: offered deadline missed", phase);
    CHECK_RET(rclc_publisher_set_offered_deadline_missed_callback(
                  pub, 500, on_pub_count, &s_ctx_ids[K_OFFERED_DEADLINE_MISSED]),
              expected, msg);
}

int main(void) {
    setenv("NROS_RMW", NROS_STUB_RMW_NAME, 1);
    nros_stub_rmw_set_accept_entities(true);

    struct rclc_support_t support = rclc_support_get_zero_initialized();
    CHECK_RET(rclc_support_init_rmw(&support, "stub://none", 48, "status_event_setters",
                                    NROS_STUB_RMW_NAME),
              RCL_RET_OK, "support opened on the stub backend");
    if (s_failures != 0) return 1;

    rclc_executor_t executor = rclc_executor_get_zero_initialized_executor();
    CHECK_RET(rclc_executor_init(&executor, &support.context, 4, NULL), RCL_RET_OK,
              "executor initialised");
    rcl_node_options_t opts = rcl_node_get_default_options();
    rcl_node_t node = rcl_get_zero_initialized_node();
    CHECK_RET(rclc_executor_node_init(&executor, &node, "status_events", &opts), RCL_RET_OK,
              "node bound to the executor");

    rcl_publisher_t pub = rcl_get_zero_initialized_publisher();
    CHECK_RET(rclc_publisher_init_default(&pub, &node, &MSG_TYPE, "/status_events"), RCL_RET_OK,
              "publisher created");
    rcl_subscription_t sub = rcl_get_zero_initialized_subscription();
    CHECK_RET(rclc_subscription_init_default(&sub, &node, &MSG_TYPE, "/status_events"),
              RCL_RET_OK, "subscription created");
    if (s_failures != 0) return 1;

    /* ---- 3. an unattached subscription has no subscriber to ask ---- */
    CHECK_RET(rclc_subscription_set_liveliness_changed_callback(
                  &sub, on_liveliness_changed, &s_ctx_ids[K_LIVELINESS_CHANGED]),
              RCL_RET_NOT_INIT, "unattached subscription: NOT_INIT");
    CHECK_RET(rclc_subscription_set_requested_deadline_missed_callback(
                  &sub, 250, on_sub_count, &s_ctx_ids[K_REQUESTED_DEADLINE_MISSED]),
              RCL_RET_NOT_INIT, "unattached subscription: NOT_INIT");
    CHECK_RET(rclc_subscription_set_message_lost_callback(&sub, on_sub_count,
                                                          &s_ctx_ids[K_MESSAGE_LOST]),
              RCL_RET_NOT_INIT, "unattached subscription: NOT_INIT");

    CHECK_RET(rclc_executor_add_subscription_raw(&executor, &sub, on_message, NULL, ON_NEW_DATA),
              RCL_RET_OK, "subscription added to the executor");

    /* Caller errors. */
    CHECK_RET(rclc_publisher_set_liveliness_lost_callback(&pub, NULL, NULL),
              RCL_RET_INVALID_ARGUMENT, "NULL callback is refused");
    CHECK_RET(rclc_subscription_set_message_lost_callback(NULL, on_sub_count, NULL),
              RCL_RET_INVALID_ARGUMENT, "NULL subscription is refused");

    /* Negative control: nothing registered yet, nothing to fire. */
    CHECK(!nros_stub_rmw_fire_liveliness_changed(1, 0, 1, 0),
          "negative control: no liveliness registration before the setter");
    for (int k = K_REQUESTED_DEADLINE_MISSED; k < K_COUNT; k++) {
        CHECK(!nros_stub_rmw_fire_count_event(k, 1, 1),
              "negative control: no count registration before the setter");
    }

    /* ---- 4. a backend without status events is reported, not accepted ---- */
    nros_stub_rmw_set_status_events_supported(false);
    register_all(&sub, &pub, RCL_RET_UNSUPPORTED, "no event support");
    nros_stub_rmw_set_status_events_supported(true);
    for (int k = 0; k < K_COUNT; k++) {
        CHECK(nros_stub_rmw_status_event_deadline_ms(k) == -1,
              "an UNSUPPORTED answer registered nothing in the backend");
    }

    /* ---- 1. every setter registers, with the deadline it was given ---- */
    register_all(&sub, &pub, RCL_RET_OK, "supported");
    CHECK(nros_stub_rmw_status_event_deadline_ms(K_REQUESTED_DEADLINE_MISSED) == 250,
          "requested-deadline registration carries the caller's deadline_ms");
    CHECK(nros_stub_rmw_status_event_deadline_ms(K_OFFERED_DEADLINE_MISSED) == 500,
          "offered-deadline registration carries the caller's deadline_ms");
    CHECK(nros_stub_rmw_status_event_deadline_ms(K_LIVELINESS_CHANGED) == 0,
          "liveliness-changed registration carries deadline 0");
    CHECK(nros_stub_rmw_status_event_deadline_ms(K_MESSAGE_LOST) == 0,
          "message-lost registration carries deadline 0");
    CHECK(nros_stub_rmw_status_event_deadline_ms(K_LIVELINESS_LOST) == 0,
          "liveliness-lost registration carries deadline 0");

    /* ---- 2. a fire reaches the user callback with entity, status, ctx ---- */
    CHECK(nros_stub_rmw_fire_liveliness_changed(2, 1, 1, -1), "liveliness registration held");
    {
        const seen_t* s = &s_seen[K_LIVELINESS_CHANGED];
        CHECK(s->calls == 1, "liveliness changed: callback fired once");
        CHECK(s->entity == (void*)&sub, "liveliness changed: entity is the subscription");
        CHECK(s->ctx == (void*)&s_ctx_ids[K_LIVELINESS_CHANGED], "liveliness changed: context");
        CHECK(s->live.alive_count == 2 && s->live.not_alive_count == 1 &&
                  s->live.alive_count_change == 1 && s->live.not_alive_count_change == -1,
              "liveliness changed: status fields arrive intact");
    }
    for (int k = K_REQUESTED_DEADLINE_MISSED; k < K_COUNT; k++) {
        const uint32_t total = 100u + (uint32_t)k;
        CHECK(nros_stub_rmw_fire_count_event(k, total, 1), "count registration held");
        const seen_t* s = &s_seen[k];
        void* want = (k == K_LIVELINESS_LOST || k == K_OFFERED_DEADLINE_MISSED) ? (void*)&pub
                                                                               : (void*)&sub;
        if (s->calls != 1 || s->entity != want || s->ctx != (void*)&s_ctx_ids[k] ||
            s->count.total_count != total || s->count.total_count_change != 1) {
            fprintf(stderr,
                    "FAIL: kind %d: calls=%d entity_ok=%d ctx_ok=%d total=%u change=%u\n", k,
                    s->calls, s->entity == want, s->ctx == (void*)&s_ctx_ids[k],
                    (unsigned)s->count.total_count, (unsigned)s->count.total_count_change);
            s_failures++;
        }
    }

    /* Re-registering replaces, it does not take a second slot: the old
     * registration's tag still names the same slot, so one fire = one call. */
    CHECK_RET(rclc_subscription_set_message_lost_callback(&sub, on_sub_count,
                                                          &s_ctx_ids[K_MESSAGE_LOST]),
              RCL_RET_OK, "re-registration accepted");
    CHECK(nros_stub_rmw_fire_count_event(K_MESSAGE_LOST, 7, 1), "message lost re-fired");
    CHECK(s_seen[K_MESSAGE_LOST].calls == 2, "re-registration: one fire, one call");

    /* Removing the subscription from its executor drops the subscriber the
     * slots are keyed on, so it releases them too: a fire is now a no-op, and
     * a re-add can register again. */
    CHECK_RET(rclc_executor_remove_subscription(&executor, &sub), RCL_RET_OK,
              "subscription removed from the executor");
    CHECK(nros_stub_rmw_fire_count_event(K_MESSAGE_LOST, 8, 1), "message lost fired after remove");
    CHECK(s_seen[K_MESSAGE_LOST].calls == 2, "remove released the subscription's slots");
    CHECK_RET(rclc_executor_add_subscription_raw(&executor, &sub, on_message, NULL, ON_NEW_DATA),
              RCL_RET_OK, "subscription re-added");
    CHECK_RET(rclc_subscription_set_message_lost_callback(&sub, on_sub_count,
                                                          &s_ctx_ids[K_MESSAGE_LOST]),
              RCL_RET_OK, "registration after re-add");
    CHECK_RET(rclc_subscription_set_liveliness_changed_callback(
                  &sub, on_liveliness_changed, &s_ctx_ids[K_LIVELINESS_CHANGED]),
              RCL_RET_OK, "registration after re-add");
    CHECK(nros_stub_rmw_fire_count_event(K_MESSAGE_LOST, 9, 1), "message lost fired after re-add");
    CHECK(s_seen[K_MESSAGE_LOST].calls == 3, "re-added subscription's registration fires");

    /* ---- 5. a late fire after fini is a no-op ---- */
    const int before = total_calls();
    CHECK_RET(rcl_publisher_fini(&pub, &node), RCL_RET_OK, "publisher fini");
    CHECK_RET(rcl_subscription_fini(&sub, &node), RCL_RET_OK, "subscription fini");
    CHECK(nros_stub_rmw_fire_count_event(K_LIVELINESS_LOST, 9, 1),
          "the backend still holds the publisher's registration after fini");
    CHECK(nros_stub_rmw_fire_count_event(K_OFFERED_DEADLINE_MISSED, 9, 1),
          "the backend still holds the publisher's registration after fini");
    CHECK(nros_stub_rmw_fire_count_event(K_MESSAGE_LOST, 9, 1),
          "the backend still holds the subscription's registration after fini");
    CHECK(nros_stub_rmw_fire_liveliness_changed(0, 2, -2, 1),
          "the backend still holds the subscription's registration after fini");
    if (total_calls() != before) {
        fprintf(stderr, "FAIL: %d callback(s) ran after fini\n", total_calls() - before);
        s_failures++;
    }

    (void)rcl_node_fini(&node);
    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (s_failures != 0) {
        fprintf(stderr, "status_event_setters: %d failure(s)\n", s_failures);
        return 1;
    }
    printf("status_event_setters: OK (5 setters registered with their deadlines and fired; "
           "NOT_INIT unattached; UNSUPPORTED without backend support; late fire after fini is a "
           "no-op)\n");
    return 0;
}
