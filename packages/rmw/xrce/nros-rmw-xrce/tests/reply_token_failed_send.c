/* Issue 2577 — a reply the session cannot buffer must give its token back.
 *
 * `take_request` moves a request's `SampleIdentity` into one of
 * `XRCE_MAX_PENDING_REPLIES` reply tokens and hands the index out as `seq`;
 * `send_response(seq)` reads it back. When `uxr_buffer_reply` fails (the
 * reliable output stream is full), the executor does not retry, so a token the
 * failure path keeps is gone for good: after `XRCE_MAX_PENDING_REPLIES`
 * failures every `take_request` answers WOULD_BLOCK and the service is dead for
 * the life of the process. The reporter measured exactly that on hardware:
 * 4 failed sends, then 9743 WOULD_BLOCKs.
 *
 * NO AGENT: the session is zeroed, so it has no output streams and every
 * `uxr_buffer_reply` fails the way a full stream does. Requests are injected
 * through `xrce_request_callback`, as `new_data_callbacks.c` does. The loop runs
 * more rounds than there are tokens, so a leak shows as a refused take.
 */

#include "internal.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

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
static uint8_t g_req_ring[XRCE_SERVICE_REQUEST_RING_DEPTH][64];

static void request_arrival(xrce_session_state_t* st, uint16_t replier_id) {
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, g_payload, sizeof(g_payload));
    SampleIdentity sid;
    memset(&sid, 0, sizeof(sid));
    xrce_request_callback(NULL, uxr_object_id(replier_id, UXR_REPLIER_ID), 0, &sid, &ub,
                          sizeof(g_payload), st);
}

static int tokens_in_use(const xrce_service_server_slot* slot) {
    int n = 0;
    for (int i = 0; i < XRCE_MAX_PENDING_REPLIES; ++i) {
        n += slot->reply_tokens[i].in_use ? 1 : 0;
    }
    return n;
}

int main(void) {
    printf("issue 2577: a reply the session cannot buffer releases its token\n");
    CHECK(nros_rmw_xrce_register() == NROS_RMW_RET_OK && g_vtable != NULL, "vtable registered");
    if (g_vtable == NULL) {
        return 1;
    }

    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        return 1;
    }
    xrce_service_server_slot* slot = &st->service_server_slots[0];
    slot->active = true;
    slot->replier_id = 9;
    slot->ring_bytes = &g_req_ring[0][0];
    slot->entry_cap = sizeof(g_req_ring[0]);
    xrce_service_server_state ss;
    memset(&ss, 0, sizeof(ss));
    ss.session_state = st;
    ss.slot = slot;
    rmw_service_t srv;
    memset(&srv, 0, sizeof(srv));
    srv.backend_data = &ss;

    /* Twice as many requests as tokens: with a leak the take after the last
     * token fails; without one every round takes and every send fails. */
    const int rounds = 2 * XRCE_MAX_PENDING_REPLIES;
    int taken_rounds = 0;
    int failed_sends = 0;
    for (int r = 0; r < rounds; ++r) {
        request_arrival(st, 9);
        uint8_t buf[64];
        rmw_mut_byte_span_t span = {buf, sizeof(buf), NROS_RMW_TAKE_LEN_UNKNOWN};
        int64_t seq = -1;
        bool taken = false;
        rmw_ret_t rc = xrce_service_take_request(&srv, &span, &seq, &taken);
        if (rc != NROS_RMW_RET_OK || !taken) {
            printf("        round %d: take_request refused (rc %d), %d token(s) in use\n", r,
                   (int)rc, tokens_in_use(slot));
            break;
        }
        taken_rounds++;
        uint8_t reply[8] = {0, 1, 0, 0, 7, 0, 0, 0};
        rmw_byte_span_t resp = {reply, sizeof(reply)};
        if (xrce_service_send_response(&srv, seq, resp) != NROS_RMW_RET_OK) {
            failed_sends++;
        }
        /* The request this round left in the ring (if the take refused) is
         * not ours to drain; the take above consumed it otherwise. */
    }
    printf("        %d of %d rounds taken, %d send(s) failed, %d token(s) still in use\n",
           taken_rounds, rounds, failed_sends, tokens_in_use(slot));
    CHECK(failed_sends == taken_rounds, "every send fails (the session buffers nothing)");
    CHECK(taken_rounds == rounds, "every request is still taken after the failed sends");
    CHECK(tokens_in_use(slot) == 0, "no reply token is left in use");

    nros_xrce_free(st);
    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
