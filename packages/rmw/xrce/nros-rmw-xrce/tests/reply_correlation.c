/* Issue 2026 — a reply reports the id of the request it ANSWERS.
 *
 * Micro-XRCE's `on_reply` hands the callback two ids. Its `request_id` is the
 * id of the `uxr_buffer_request_data` read that delivered the reply; the id
 * `uxr_buffer_request` returned when the request was sent arrives as
 * `reply_id`. `xrce_reply_callback` recorded the first, which nothing compared
 * until issue 2009 made every client drop a reply naming no pending request —
 * and then every XRCE service client dropped every reply.
 *
 * NO AGENT: the callback is called directly, with the two ids deliberately
 * different, the way `read_access.c` calls it.
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

int main(void) {
    printf("issue 2026: an XRCE reply reports the id its request was sent with\n");

    xrce_session_state_t* st = (xrce_session_state_t*)calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        return 1;
    }
    xrce_service_client_slot* slot = &st->service_client_slots[0];
    slot->active = true;
    slot->requester_id = 7;

    /* The id `uxr_buffer_request` returned for the request, and the id of the
     * read that brought its reply back. They differ in every real session:
     * the read is requested once, the requests count up. */
    const uint16_t sent_with = 42;
    const uint16_t read_id = 3;

    uint8_t payload[8] = {0, 1, 0, 0, 5, 0, 0, 0};
    ucdrBuffer ub;
    ucdr_init_buffer(&ub, payload, sizeof(payload));
    uxrObjectId requester = {.id = 7, .type = UXR_REQUESTER_ID};
    xrce_reply_callback(NULL, requester, read_id, sent_with, &ub, sizeof(payload), st);
    CHECK(slot->has_reply, "the reply is staged in the requester's slot");

    xrce_service_client_state state;
    memset(&state, 0, sizeof(state));
    state.slot = slot;
    rmw_client_t cli;
    memset(&cli, 0, sizeof(cli));
    cli.backend_data = &state;

    uint8_t buf[XRCE_SERVICE_REPLY_BUFFER_SIZE];
    rmw_mut_byte_span_t span = {buf, sizeof(buf), NROS_RMW_TAKE_LEN_UNKNOWN};
    int64_t seq = -1;
    bool taken = false;
    rmw_ret_t rc = xrce_service_take_response(&cli, &span, &seq, &taken);
    CHECK(rc == NROS_RMW_RET_OK && taken, "take_response hands the reply over");
    printf("        sent with %u, read %u, reported %lld\n", sent_with, read_id, (long long)seq);
    CHECK(seq == sent_with, "and reports the id the request was SENT with");

    free(st);
    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
