/* Issue 2135 — `received_timestamp` on a taken sample and a taken request.
 *
 * No agent. An arrival is injected through the callbacks micro-XRCE-DDS-Client
 * calls from inside `drive_io` (`xrce_topic_callback`, `xrce_request_callback`),
 * which is where the backend stamps it, and the take must report THAT time: on
 * the platform wall clock (`nros_platform_time_now_ns`, upstream's
 * `system_clock`), inside the window around the injection, and not the time of
 * the take. Every other message-info field stays "not reported".
 */

#include "internal.h"
#include "nros/platform.h"
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

static uint8_t g_payload[8] = {0, 1, 0, 0, 5, 0, 0, 0};
static uint8_t g_req_ring[XRCE_SERVICE_REQUEST_RING_DEPTH > 0 ? XRCE_SERVICE_REQUEST_RING_DEPTH
                                                              : 1][64];

static int64_t now_ns(void) {
    return (int64_t)nros_platform_time_now_ns();
}

/* Long enough that a take stamping its own time would land outside the
 * injection window. */
static void pause_ms(int64_t ms) {
    const int64_t until = now_ns() + ms * 1000000LL;
    while (now_ns() < until) {
    }
}

int main(void) {
    printf("received_timestamp\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL) {
        return 1;
    }
    CHECK(g_vtable->take_with_info != NULL, "the take_with_info slot is filled");
    CHECK(g_vtable->take_request_with_info != NULL, "the take_request_with_info slot is filled");
    if (failures != 0) {
        return 1;
    }
    CHECK(now_ns() > 0, "this host has a wall clock (else every stamp is 0, 'not stamped')");

    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        return 1;
    }

    /* ---- subscription ---- */
    xrce_subscriber_slot* sslot = &st->subscriber_slots[0];
    sslot->active = true;
    sslot->datareader_id = 5;
    xrce_subscriber_state ss;
    memset(&ss, 0, sizeof(ss));
    ss.session_state = st;
    ss.slot = sslot;
    rmw_subscription_t sub;
    memset(&sub, 0, sizeof(sub));
    sub.backend_data = &ss;

    int64_t before = now_ns();
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    uxrStreamId stream = {0};
    xrce_topic_callback(NULL, uxr_object_id(5, UXR_DATAREADER_ID), 0, stream, &ub,
                        sizeof(g_payload), st);
    int64_t after = now_ns();
    pause_ms(50);

    uint8_t buf[64];
    rmw_mut_byte_span_t span = {buf, sizeof(buf), 0};
    bool taken = false;
    rmw_message_info_t info;
    memset(&info, 0x5a, sizeof(info));
    rmw_ret_t rc = g_vtable->take_with_info(&sub, &span, &taken, &info);
    CHECK(rc == NROS_RMW_RET_OK && taken && span.len > 0,
          "subscription: the sample is taken");
    printf("        window [%lld, %lld], received %lld\n", (long long)before, (long long)after,
           (long long)info.received_timestamp);
    CHECK(info.received_timestamp >= before && info.received_timestamp <= after,
          "subscription: received_timestamp is the ARRIVAL, inside the injection window "
          "(not the take, 50 ms later)");
    CHECK(info.source_timestamp == 0, "subscription: source_timestamp not reported (0)");
    CHECK(info.publication_sequence_number == RMW_MESSAGE_INFO_SEQUENCE_NUMBER_UNSUPPORTED &&
              info.reception_sequence_number == RMW_MESSAGE_INFO_SEQUENCE_NUMBER_UNSUPPORTED,
          "subscription: both sequence numbers UNSUPPORTED");
    uint8_t zero_gid[sizeof(info.publisher_gid.data)];
    memset(zero_gid, 0, sizeof(zero_gid));
    CHECK(memcmp(info.publisher_gid.data, zero_gid, sizeof(zero_gid)) == 0 &&
              !info.from_intra_process,
          "subscription: no publisher gid, not intra-process");

    /* An empty take writes nothing. */
    memset(&info, 0x5a, sizeof(info));
    taken = true;
    rc = g_vtable->take_with_info(&sub, &span, &taken, &info);
    CHECK(rc == NROS_RMW_RET_OK && !taken && info.received_timestamp == 0x5a5a5a5a5a5a5a5aLL,
          "subscription: an empty take is OK, not taken, info untouched");
    CHECK(g_vtable->take_with_info(&sub, &span, &taken, NULL) == NROS_RMW_RET_INVALID_ARGUMENT,
          "subscription: NULL info is INVALID_ARGUMENT");

    /* ---- service request ---- */
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

    SampleIdentity sid;
    memset(&sid, 0, sizeof(sid));
    sid.writer_guid.guidPrefix.data[0] = 0x01;
    sid.sequence_number.low = 3;
    before = now_ns();
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    xrce_request_callback(NULL, uxr_object_id(9, UXR_REPLIER_ID), 0, &sid, &ub,
                          sizeof(g_payload), st);
    after = now_ns();
    pause_ms(50);

    rmw_service_info_t sinfo;
    memset(&sinfo, 0x5a, sizeof(sinfo));
    int64_t seq = -1;
    span.capacity = sizeof(buf);
    span.len = 0;
    rc = g_vtable->take_request_with_info(&srv, &span, &seq, &taken, &sinfo);
    CHECK(rc == NROS_RMW_RET_OK && taken, "service: the request is taken");
    printf("        window [%lld, %lld], received %lld\n", (long long)before, (long long)after,
           (long long)sinfo.received_timestamp);
    CHECK(sinfo.received_timestamp >= before && sinfo.received_timestamp <= after,
          "service: received_timestamp is the ARRIVAL, inside the injection window");
    CHECK(sinfo.source_timestamp == 0, "service: source_timestamp not reported (0)");
    CHECK(sinfo.request_id.sequence_number == 3 && sinfo.request_id.writer_guid[0] == 0x01,
          "service: the request id is still the SampleIdentity's");

    nros_xrce_free(st);
    if (failures != 0) {
        printf("received_timestamp: %d failure(s)\n", failures);
        return 1;
    }
    printf("received_timestamp: OK\n");
    return 0;
}
