/* Issue 1632 — a too-small take_request / take_response names the size the
 * request or reply needed.
 *
 * `take_refused_len.c` (issue 1612) is the subscription half. The span rule is
 * the same on the two service slots: on `NROS_RMW_RET_BUFFER_TOO_SMALL` the
 * span's `len` is the size the payload needed.
 *
 * NO AGENT: the request ring and the reply slot are filled directly, the way
 * `xrce_request_callback` / `xrce_reply_callback` fill them, because what is
 * under test is what the TAKE reports about a payload it cannot hand over.
 */

#include "internal.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <stdio.h>
#include <stdlib.h>
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

static void stage_request(xrce_service_server_slot* slot, size_t len) {
    xrce_service_request_entry* e = &slot->req_ring[slot->req_write_idx];
    memset(e->data, 0xcd, len);
    e->len = len;
    e->overflow = false;
    slot->req_write_idx = (uint16_t)((slot->req_write_idx + 1) % XRCE_SERVICE_REQUEST_RING_DEPTH);
    slot->req_count++;
}

static void request_half(void) {
    const size_t request =
        XRCE_SERVICE_REQUEST_BUFFER_SIZE < 300 ? XRCE_SERVICE_REQUEST_BUFFER_SIZE : 300;
    const size_t cap = request / 2;

    xrce_service_server_slot* slot =
        (xrce_service_server_slot*)calloc(1, sizeof(xrce_service_server_slot));
    if (slot == NULL) {
        printf("  FAIL: could not allocate a server slot\n");
        failures++;
        return;
    }
    slot->active = true;
    xrce_service_server_state state;
    memset(&state, 0, sizeof(state));
    state.slot = slot;
    rmw_service_t srv;
    memset(&srv, 0, sizeof(srv));
    srv.backend_data = &state;

    uint8_t buf[XRCE_SERVICE_REQUEST_BUFFER_SIZE];
    int64_t seq = -1;
    bool taken = false;

    stage_request(slot, request);
    rmw_mut_byte_span_t span = {buf, cap, NROS_RMW_TAKE_LEN_UNKNOWN};
    rmw_ret_t rc = xrce_service_take_request(&srv, &span, &seq, &taken);
    CHECK(rc == NROS_RMW_RET_BUFFER_TOO_SMALL, "a request over the buffer is BUFFER_TOO_SMALL");
    CHECK(span.len == request, "and `len` names the size the request needed");
    printf("        request %zu bytes, buffer %zu, reported len %zu\n", request, cap, span.len);
    CHECK(slot->req_count == 0, "the refused request is consumed, so the ring cannot wedge");

    stage_request(slot, request);
    span.capacity = sizeof(buf);
    span.len = NROS_RMW_TAKE_LEN_UNKNOWN;
    rc = xrce_service_take_request(&srv, &span, &seq, &taken);
    CHECK(rc == NROS_RMW_RET_OK && taken && span.len == request,
          "a buffer that fits takes the request, same length");
    free(slot);
}

static void reply_half(void) {
    const size_t reply =
        XRCE_SERVICE_REPLY_BUFFER_SIZE < 300 ? XRCE_SERVICE_REPLY_BUFFER_SIZE : 300;
    const size_t cap = reply / 2;

    xrce_service_client_slot* slot =
        (xrce_service_client_slot*)calloc(1, sizeof(xrce_service_client_slot));
    if (slot == NULL) {
        printf("  FAIL: could not allocate a client slot\n");
        failures++;
        return;
    }
    slot->active = true;
    xrce_service_client_state state;
    memset(&state, 0, sizeof(state));
    state.slot = slot;
    rmw_client_t cli;
    memset(&cli, 0, sizeof(cli));
    cli.backend_data = &state;

    uint8_t buf[XRCE_SERVICE_REPLY_BUFFER_SIZE];
    int64_t seq = -1;
    bool taken = false;

    memset(slot->data, 0xef, reply);
    slot->len = reply;
    slot->has_reply = true;
    rmw_mut_byte_span_t span = {buf, cap, NROS_RMW_TAKE_LEN_UNKNOWN};
    rmw_ret_t rc = xrce_service_take_response(&cli, &span, &seq, &taken);
    CHECK(rc == NROS_RMW_RET_BUFFER_TOO_SMALL, "a reply over the buffer is BUFFER_TOO_SMALL");
    CHECK(span.len == reply, "and `len` names the size the reply needed");
    printf("        reply %zu bytes, buffer %zu, reported len %zu\n", reply, cap, span.len);
    CHECK(!slot->has_reply, "the refused reply is consumed");

    slot->has_reply = true;
    span.capacity = sizeof(buf);
    span.len = NROS_RMW_TAKE_LEN_UNKNOWN;
    rc = xrce_service_take_response(&cli, &span, &seq, &taken);
    CHECK(rc == NROS_RMW_RET_OK && taken && span.len == reply,
          "a buffer that fits takes the reply, same length");
    free(slot);
}

int main(void) {
    printf("issue 1632: a too-small XRCE service take reports the payload's size\n");
    request_half();
    reply_half();
    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
