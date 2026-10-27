/* Issue 2108 — client-side DEADLINE emulation.
 *
 * No agent. A subscription with a 50 ms requested deadline and no arrivals
 * reports REQUESTED_DEADLINE_MISSED with the right `total_count_change`, once
 * per check that saw whole periods end; an arrival restarts the period;
 * `next_deadline_ms` names the earliest expiry; a publisher's offered deadline
 * is restarted by publishing and its destroy takes it off the session's list.
 * Time is driven through `xrce_deadline_check(st, now_ms)`, never slept.
 */

#include "internal.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, what)                                                                          \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            printf("  FAIL: %s\n", (what));                                                        \
            failures++;                                                                            \
        } else {                                                                                   \
            printf("  ok    %s\n", (what));                                                        \
        }                                                                                          \
    } while (0)

static const nros_rmw_vtable_t* g_vtable = NULL;
rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    g_vtable = vtable;
    return NROS_RMW_RET_OK;
}

typedef struct {
    int calls;
    rmw_event_type_t kind;
    uint32_t total;
    uint32_t change;
    void* ctx;
} seen_t;

static void on_event(rmw_event_type_t kind, const rmw_event_payload_t* p, void* ctx) {
    seen_t* s = (seen_t*)ctx;
    s->calls++;
    s->kind = kind;
    s->total = p->count.total_count;
    s->change = p->count.total_count_change;
    s->ctx = ctx;
}

int main(void) {
    printf("client-side DEADLINE emulation\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL) {
        return 1;
    }
    CHECK(g_vtable->subscription_event_init != NULL && g_vtable->publisher_event_init != NULL &&
              g_vtable->next_deadline_ms != NULL && g_vtable->event_type_is_supported != NULL,
          "event_init x2, next_deadline_ms and event_type_is_supported are filled");
    if (failures != 0) {
        return 1;
    }
    CHECK(g_vtable->event_type_is_supported(NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED) &&
              g_vtable->event_type_is_supported(NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED) &&
              !g_vtable->event_type_is_supported(NROS_RMW_EVENT_LIVELINESS_CHANGED) &&
              !g_vtable->event_type_is_supported(NROS_RMW_EVENT_MESSAGE_LOST) &&
              !g_vtable->event_type_is_supported(NROS_RMW_EVENT_SUBSCRIPTION_MATCHED),
          "event_type_is_supported: the two DEADLINE kinds, nothing else");
    uint32_t mask = 0;
    CHECK(g_vtable->supported_qos_policies(NULL, &mask) == NROS_RMW_RET_OK &&
              (mask & NROS_RMW_QOS_POLICY_DEADLINE) != 0,
          "the DEADLINE QoS bit is advertised (earned by the emulation)");

    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        return 1;
    }
    rmw_session_t session;
    memset(&session, 0, sizeof(session));
    session.backend_data = st;

    /* ---- subscription, 50 ms ---- */
    xrce_subscriber_slot* slot = &st->subscriber_slots[0];
    slot->active = true;
    slot->datareader_id = 5;
    xrce_subscriber_state ss;
    memset(&ss, 0, sizeof(ss));
    ss.session_state = st;
    ss.slot = slot;
    rmw_subscription_t sub;
    memset(&sub, 0, sizeof(sub));
    sub.backend_data = &ss;

    seen_t s;
    memset(&s, 0, sizeof(s));
    CHECK(g_vtable->subscription_event_init(&sub, NROS_RMW_EVENT_LIVELINESS_CHANGED, 0, on_event,
                                            &s) == NROS_RMW_RET_UNSUPPORTED,
          "a non-deadline kind is UNSUPPORTED");
    CHECK(g_vtable->subscription_event_init(&sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, 50,
                                            on_event, &s) == NROS_RMW_RET_OK,
          "register REQUESTED_DEADLINE_MISSED, 50 ms");
    const uint64_t t0 = slot->deadline.next_expiry_ms - 50u; /* the registration's clock */

    uint32_t ms = 0;
    bool has = false;
    CHECK(xrce_deadline_next(st, t0 + 10u, &ms) && ms == 40u,
          "next_deadline_ms names the expiry (40 ms left, 10 ms in)");
    CHECK(g_vtable->next_deadline_ms(&session, &ms, &has) == NROS_RMW_RET_OK && has && ms <= 50u,
          "the slot answers a deadline, at most one period away");

    xrce_deadline_check(st, t0 + 49u);
    CHECK(s.calls == 0, "no miss before the period ends");
    xrce_deadline_check(st, t0 + 50u);
    CHECK(s.calls == 1 && s.kind == NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED && s.total == 1 &&
              s.change == 1 && s.ctx == &s,
          "50 ms with no arrival: one miss, total 1, change 1, the registering context");
    xrce_deadline_check(st, t0 + 99u);
    CHECK(s.calls == 1, "the next period has not ended yet");
    xrce_deadline_check(st, t0 + 260u);
    CHECK(s.calls == 2 && s.total == 5 && s.change == 4,
          "a check 260 ms in counts every whole period: total 5, change 4 (one call)");
    CHECK(xrce_deadline_next(st, t0 + 260u, &ms) && ms == 40u,
          "next expiry is the end of the current period (300 ms)");

    /* An arrival restarts the period: make the current one already overdue,
     * then deliver a sample — the next expiry must move to a period after the
     * arrival, so a check just inside that period finds nothing. */
    slot->deadline.next_expiry_ms = 1u;
    const uint64_t before_arrival = xrce_now_ms();
    uint8_t payload[8] = {0, 1, 0, 0, 5, 0, 0, 0};
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, payload, sizeof(payload));
    uxrStreamId stream = {0};
    xrce_topic_callback(NULL, uxr_object_id(5, UXR_DATAREADER_ID), 0, stream, &ub,
                        sizeof(payload), st);
    const uint64_t t_arr = slot->deadline.next_expiry_ms - 50u;
    CHECK(slot->count == 1 && t_arr >= before_arrival,
          "an arrival restarts the period from the arrival");
    xrce_deadline_check(st, before_arrival + 49u);
    CHECK(s.calls == 2, "no miss within a period of the arrival");

    /* Disarm: a NULL callback clears it. */
    CHECK(g_vtable->subscription_event_init(&sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, 50,
                                            NULL, NULL) == NROS_RMW_RET_OK &&
              slot->deadline.period_ms == 0,
          "a NULL callback disarms");
    CHECK(!xrce_deadline_next(st, t_arr, &ms), "nothing armed: no deadline");

    /* The profile's deadline is used when the registration names none. */
    ss.qos_deadline_ms = 30;
    CHECK(g_vtable->subscription_event_init(&sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, 0,
                                            on_event, &s) == NROS_RMW_RET_OK &&
              slot->deadline.period_ms == 30,
          "deadline_ms 0 falls back to the profile's deadline");
    ss.qos_deadline_ms = NROS_RMW_DURATION_INFINITE_MS;
    (void)g_vtable->subscription_event_init(&sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, 0,
                                            on_event, &s);
    CHECK(slot->deadline.period_ms == 0, "an infinite deadline arms nothing");

    /* ---- publisher, offered 20 ms ---- */
    xrce_publisher_state* ps =
        (xrce_publisher_state*)nros_xrce_calloc(1, sizeof(xrce_publisher_state));
    if (ps == NULL) {
        return 1;
    }
    ps->session_state = st;
    rmw_publisher_t pub;
    memset(&pub, 0, sizeof(pub));
    pub.backend_data = ps;
    seen_t p;
    memset(&p, 0, sizeof(p));
    CHECK(g_vtable->publisher_event_init(&pub, NROS_RMW_EVENT_LIVELINESS_LOST, 0, on_event, &p) ==
              NROS_RMW_RET_UNSUPPORTED,
          "publisher: a non-deadline kind is UNSUPPORTED");
    CHECK(g_vtable->publisher_event_init(&pub, NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED, 20,
                                         on_event, &p) == NROS_RMW_RET_OK &&
              st->deadline_publishers == ps,
          "publisher: register OFFERED_DEADLINE_MISSED, listed on the session");
    const uint64_t p0 = ps->deadline.next_expiry_ms - 20u;
    xrce_deadline_check(st, p0 + 45u);
    CHECK(p.calls == 1 && p.kind == NROS_RMW_EVENT_OFFERED_DEADLINE_MISSED && p.total == 2 &&
              p.change == 2,
          "publisher: 45 ms without a publish = two missed periods");
    xrce_deadline_touch(&ps->deadline, p0 + 50u); /* what a publish does */
    xrce_deadline_check(st, p0 + 69u);
    CHECK(p.calls == 1, "publisher: a publish restarts the period");

    /* Destroy unlists it (the session is closed, so no transport is touched). */
    st->session_closed = true;
    st->live_entities = 2;
    CHECK(xrce_publisher_destroy(&pub) == NROS_RMW_RET_OK && st->deadline_publishers == NULL,
          "publisher: destroy takes it off the deadline list");

    nros_xrce_free(st);
    if (failures != 0) {
        printf("deadline: %d failure(s)\n", failures);
        return 1;
    }
    printf("deadline: OK\n");
    return 0;
}
