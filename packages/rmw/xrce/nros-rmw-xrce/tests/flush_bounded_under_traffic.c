/* Issue 2578 — a flush returns within its window while inbound bytes keep coming.
 *
 * The pinned Micro-XRCE-DDS-Client's `uxr_run_session_time(s, ms)` loops
 * `while (!timeout) timeout = !listen_message_reliably(s, ms);`, so every window
 * that receives ANYTHING starts another: under steady inbound traffic (a 20 Hz
 * subscription) it never returns, and the executor stops dispatching every
 * callback for as long as the traffic lasts. The reporter measured 33 s of a
 * 42 s run stalled.
 *
 * This drives the backend's own reply path (`xrce_service_send_response`, which
 * flushes after buffering the reply) over a custom transport whose `read` hands
 * back a few bytes every 10 ms for `TRAFFIC_MS`, i.e. faster than any flush
 * window. Any bytes count as "received" to the client, so content does not
 * matter. The flush must return within its own bound, long before the traffic
 * stops. The custom transport is installed straight into the session state,
 * so NO AGENT is needed: nothing ever acknowledges the reply, which is the
 * worst case for a flush (it waits the whole window).
 */

#include "internal.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <stdio.h>
#include <string.h>

#define TRAFFIC_MS 3000
#define TRICKLE_MS 10

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

rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    (void)vtable;
    return NROS_RMW_RET_OK;
}

/* The platform clock and sleep the backend itself paces with. */
static long long now_ms(void) {
    return (long long)(nros_platform_clock_ns() / 1000000u);
}

static void sleep_ms(long ms) {
    nros_platform_sleep_ms((uint32_t)ms);
}

static long long g_traffic_until = 0;
static int g_reads_with_bytes = 0;

static bool t_open(uxrCustomTransport* t) {
    (void)t;
    return true;
}
static bool t_close(uxrCustomTransport* t) {
    (void)t;
    return true;
}
static size_t t_write(uxrCustomTransport* t, const uint8_t* buf, size_t len, uint8_t* err) {
    (void)t;
    (void)buf;
    (void)err;
    return len; /* the "Agent" takes everything and acknowledges nothing */
}
/* A steady trickle: a few bytes every TRICKLE_MS until the traffic stops, then
 * silence for the caller's whole timeout. */
static size_t t_read(uxrCustomTransport* t, uint8_t* buf, size_t len, int timeout, uint8_t* err) {
    (void)t;
    (void)err;
    if (now_ms() < g_traffic_until && len >= 8) {
        sleep_ms(TRICKLE_MS);
        memset(buf, 0, 8);
        g_reads_with_bytes++;
        return 8;
    }
    sleep_ms(timeout > 0 ? timeout : 0);
    return 0;
}

static uint8_t g_req_ring[XRCE_SERVICE_REQUEST_RING_DEPTH][64];
static uint8_t g_payload[8] = {0, 1, 0, 0, 5, 0, 0, 0};

int main(void) {
    printf("issue 2578: a flush is bounded while inbound bytes keep arriving\n");
    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        return 1;
    }
    uxr_set_custom_transport_callbacks(&st->custom, false, t_open, t_close, t_write, t_read);
    CHECK(uxr_init_custom_transport(&st->custom, NULL), "custom transport up");
    uxr_init_session(&st->session, &st->custom.comm, 0x2578AAAAu);
    st->output_reliable = uxr_create_output_reliable_stream(
        &st->session, st->output_reliable_buf, sizeof(st->output_reliable_buf),
        XRCE_STREAM_HISTORY);
    st->input_reliable = uxr_create_input_reliable_stream(
        &st->session, st->input_reliable_buf, sizeof(st->input_reliable_buf), XRCE_STREAM_HISTORY);

    xrce_service_server_slot* slot = &st->service_server_slots[0];
    slot->active = true;
    slot->replier_id = 9;
    slot->ring_bytes = &g_req_ring[0][0];
    slot->entry_cap = sizeof(g_req_ring[0]);
    xrce_service_server_state ss;
    memset(&ss, 0, sizeof(ss));
    ss.session_state = st;
    ss.slot = slot;
    ss.replier_oid = uxr_object_id(9, UXR_REPLIER_ID);
    rmw_service_t srv;
    memset(&srv, 0, sizeof(srv));
    srv.backend_data = &ss;

    ucdrBuffer ub;
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    SampleIdentity sid;
    memset(&sid, 0, sizeof(sid));
    xrce_request_callback(NULL, ss.replier_oid, 0, &sid, &ub, sizeof(g_payload), st);
    uint8_t buf[64];
    rmw_mut_byte_span_t span = {buf, sizeof(buf), NROS_RMW_TAKE_LEN_UNKNOWN};
    int64_t seq = -1;
    bool taken = false;
    CHECK(xrce_service_take_request(&srv, &span, &seq, &taken) == NROS_RMW_RET_OK && taken,
          "a request is taken");

    g_traffic_until = now_ms() + TRAFFIC_MS;
    long long t0 = now_ms();
    uint8_t reply[8] = {0, 1, 0, 0, 7, 0, 0, 0};
    rmw_byte_span_t resp = {reply, sizeof(reply)};
    rmw_ret_t rc = xrce_service_send_response(&srv, seq, resp);
    long long took = now_ms() - t0;
    printf("        send_response took %lld ms (flush window %d ms, traffic %d ms, %d reads "
           "returned bytes)\n",
           took, (int)XRCE_SESSION_FLUSH_TIMEOUT_MS, TRAFFIC_MS, g_reads_with_bytes);
    CHECK(rc == NROS_RMW_RET_OK, "the reply is buffered");
    /* Twice the window is the bound: one window plus the last read in flight
     * (TRICKLE_MS) and scheduler noise, and far below TRAFFIC_MS. */
    CHECK(took < 2 * XRCE_SESSION_FLUSH_TIMEOUT_MS,
          "the flush returns within its window, not when the traffic stops");

    nros_xrce_free(st);
    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
