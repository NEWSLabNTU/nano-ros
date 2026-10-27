/* Issue 2109 — the three per-entity new-data callback slots.
 *
 * No agent. An arrival is injected through the same callbacks
 * micro-XRCE-DDS-Client calls from inside `drive_io` (`xrce_topic_callback`,
 * `xrce_request_callback`, `xrce_reply_callback`), and the registered callback
 * must fire once, with 1 and with the REGISTERING entity's user data. A
 * registration that is replaced, cleared, or destroyed with its entity never
 * fires again; an arrival for another entity, or one dropped because the ring
 * is full, fires nothing.
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

/* What each callback saw: how many calls, the last `n`, the last user data. */
typedef struct {
    int calls;
    size_t last_n;
    const void* last_user;
} seen_t;

static seen_t g_a, g_b;

static void cb_a(const void* user, size_t n) {
    g_a.calls++;
    g_a.last_n = n;
    g_a.last_user = user;
}

static void cb_b(const void* user, size_t n) {
    g_b.calls++;
    g_b.last_n = n;
    g_b.last_user = user;
}

static void reset(void) {
    memset(&g_a, 0, sizeof(g_a));
    memset(&g_b, 0, sizeof(g_b));
}

static uint8_t g_payload[8] = {0, 1, 0, 0, 5, 0, 0, 0};

static void topic_arrival(xrce_session_state_t* st, uint16_t reader_id) {
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    uxrStreamId stream = {0};
    xrce_topic_callback(NULL, uxr_object_id(reader_id, UXR_DATAREADER_ID), 0, stream, &ub,
                        sizeof(g_payload), st);
}

static void request_arrival(xrce_session_state_t* st, uint16_t replier_id) {
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    SampleIdentity sid;
    memset(&sid, 0, sizeof(sid));
    xrce_request_callback(NULL, uxr_object_id(replier_id, UXR_REPLIER_ID), 0, &sid, &ub,
                          sizeof(g_payload), st);
}

static void reply_arrival(xrce_session_state_t* st, uint16_t requester_id) {
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    xrce_reply_callback(NULL, uxr_object_id(requester_id, UXR_REQUESTER_ID), 0, 1, &ub,
                        sizeof(g_payload), st);
}

static uint8_t g_req_ring[XRCE_SERVICE_REQUEST_RING_DEPTH > 0 ? XRCE_SERVICE_REQUEST_RING_DEPTH
                                                              : 1][64];

int main(void) {
    printf("per-entity new-data callbacks\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL) {
        return 1;
    }
    CHECK(g_vtable->subscription_set_on_new_message_callback != NULL &&
              g_vtable->service_set_on_new_request_callback != NULL &&
              g_vtable->client_set_on_new_response_callback != NULL,
          "all three slots are filled");
    if (failures != 0) {
        return 1;
    }

    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        return 1;
    }
    int user_a = 1, user_b = 2;

    /* ---- subscription ---- */
    xrce_subscriber_slot* sslot = &st->subscriber_slots[0];
    sslot->active = true;
    sslot->datareader_id = 5;
    xrce_subscriber_slot* other = &st->subscriber_slots[1];
    other->active = true;
    other->datareader_id = 6;
    /* Heap, not stack: destroy frees the entity state. */
    xrce_subscriber_state* ss =
        (xrce_subscriber_state*)nros_xrce_calloc(1, sizeof(xrce_subscriber_state));
    if (ss == NULL) {
        return 1;
    }
    ss->session_state = st;
    ss->slot = sslot;
    rmw_subscription_t sub;
    memset(&sub, 0, sizeof(sub));
    sub.backend_data = ss;

    reset();
    CHECK(g_vtable->subscription_set_on_new_message_callback(&sub, cb_a, &user_a) ==
              NROS_RMW_RET_OK,
          "subscription: register");
    topic_arrival(st, 5);
    CHECK(g_a.calls == 1 && g_a.last_n == 1 && g_a.last_user == &user_a,
          "subscription: one arrival fires once, with 1 and the registering user data");
    topic_arrival(st, 6);
    CHECK(g_a.calls == 1, "subscription: another reader's arrival does not fire it");
    sslot->count = XRCE_SUBSCRIBER_RING_DEPTH; /* ring full: the arrival is dropped */
    topic_arrival(st, 5);
    CHECK(g_a.calls == 1, "subscription: a dropped arrival (ring full) fires nothing");
    sslot->count = 0;
    CHECK(g_vtable->subscription_set_on_new_message_callback(&sub, cb_b, &user_b) ==
              NROS_RMW_RET_OK,
          "subscription: replace");
    topic_arrival(st, 5);
    CHECK(g_a.calls == 1 && g_b.calls == 1 && g_b.last_user == &user_b,
          "subscription: after replace only the new pair fires");
    CHECK(g_vtable->subscription_set_on_new_message_callback(&sub, NULL, &user_b) ==
              NROS_RMW_RET_OK,
          "subscription: clear");
    topic_arrival(st, 5);
    CHECK(g_a.calls == 1 && g_b.calls == 1, "subscription: after clear nothing fires");

    /* ---- service ---- */
    xrce_service_server_slot* vslot = &st->service_server_slots[0];
    vslot->active = true;
    vslot->replier_id = 9;
    vslot->ring_bytes = &g_req_ring[0][0];
    vslot->entry_cap = sizeof(g_req_ring[0]);
    xrce_service_server_state vs;
    memset(&vs, 0, sizeof(vs));
    vs.session_state = st;
    vs.slot = vslot;
    rmw_service_t srv;
    memset(&srv, 0, sizeof(srv));
    srv.backend_data = &vs;
    reset();
    CHECK(g_vtable->service_set_on_new_request_callback(&srv, cb_a, &user_a) == NROS_RMW_RET_OK,
          "service: register");
    request_arrival(st, 9);
    CHECK(g_a.calls == 1 && g_a.last_n == 1 && g_a.last_user == &user_a,
          "service: one request fires once, with the registering user data");
    request_arrival(st, 10);
    CHECK(g_a.calls == 1, "service: another replier's request does not fire it");
    CHECK(g_vtable->service_set_on_new_request_callback(&srv, NULL, NULL) == NROS_RMW_RET_OK,
          "service: clear");
    request_arrival(st, 9);
    CHECK(g_a.calls == 1, "service: after clear nothing fires");

    /* ---- client ---- */
    xrce_service_client_slot* cslot = &st->service_client_slots[0];
    cslot->active = true;
    cslot->requester_id = 11;
    xrce_service_client_state cs;
    memset(&cs, 0, sizeof(cs));
    cs.session_state = st;
    cs.slot = cslot;
    rmw_client_t cli;
    memset(&cli, 0, sizeof(cli));
    cli.backend_data = &cs;
    reset();
    CHECK(g_vtable->client_set_on_new_response_callback(&cli, cb_b, &user_b) == NROS_RMW_RET_OK,
          "client: register");
    reply_arrival(st, 11);
    CHECK(g_b.calls == 1 && g_b.last_n == 1 && g_b.last_user == &user_b,
          "client: one reply fires once, with the registering user data");
    CHECK(g_vtable->client_set_on_new_response_callback(&cli, cb_a, &user_a) == NROS_RMW_RET_OK,
          "client: replace");
    reply_arrival(st, 11);
    CHECK(g_b.calls == 1 && g_a.calls == 1 && g_a.last_user == &user_a,
          "client: after replace only the new pair fires");

    /* ---- destroy clears ---- */
    CHECK(g_vtable->subscription_set_on_new_message_callback(&sub, cb_a, &user_a) ==
              NROS_RMW_RET_OK,
          "subscription: re-register before destroy");
    st->session_closed = true; /* no transport: destroy must not touch it */
    st->live_entities = 3;
    rmw_ret_t drc = xrce_subscription_destroy(&sub);
    CHECK(drc == NROS_RMW_RET_OK && sslot->on_new == NULL,
          "subscription: destroy clears the registration");
    sslot->active = true; /* a later reuse of the slot */
    reset();
    topic_arrival(st, 5);
    CHECK(g_a.calls == 0 && g_b.calls == 0, "...so a reuse of the slot fires nothing stale");

    CHECK(g_vtable->subscription_set_on_new_message_callback(NULL, cb_a, NULL) ==
                  NROS_RMW_RET_INVALID_ARGUMENT &&
              g_vtable->service_set_on_new_request_callback(NULL, cb_a, NULL) ==
                  NROS_RMW_RET_INVALID_ARGUMENT &&
              g_vtable->client_set_on_new_response_callback(NULL, cb_a, NULL) ==
                  NROS_RMW_RET_INVALID_ARGUMENT,
          "NULL entity is INVALID_ARGUMENT");

    nros_xrce_free(st);
    if (failures != 0) {
        printf("new_data_callbacks: %d failure(s)\n", failures);
        return 1;
    }
    printf("new_data_callbacks: OK\n");
    return 0;
}
