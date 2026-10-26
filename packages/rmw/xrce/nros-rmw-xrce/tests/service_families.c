/* Issue 1808 — the service-server families.
 *
 * The parameter and lifecycle services draw from slots of their own once the
 * cargo lane derives a split (XRCE_BUILTIN_SERVICE_SERVERS > 0), so the
 * application's request buffer can be sized from its own declared servers.
 * This checks the two halves that do not need a split compiled in: which names
 * are builtin, and that a claimed slot is bound to bytes of its family's size.
 *
 * NO AGENT: the session state is allocated and its slots claimed directly.
 */

#include "internal.h"

#include <stdio.h>
#include <stdlib.h>

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

static void names(void) {
    CHECK(xrce_is_builtin_service("/talker/set_parameters"), "a node's set_parameters is builtin");
    CHECK(xrce_is_builtin_service("/ns/robot/get_parameter_types"), "under a namespace too");
    CHECK(xrce_is_builtin_service("/lc_node/change_state"), "a lifecycle service is builtin");
    CHECK(xrce_is_builtin_service("/lc_node/get_transition_graph"), "all five of them");
    CHECK(!xrce_is_builtin_service("/set_parameters"), "no node in front: not builtin");
    CHECK(!xrce_is_builtin_service("set_parameters"), "no slash at all: not builtin");
    CHECK(!xrce_is_builtin_service("/talker/set_parameters_v2"), "a longer name is not builtin");
    CHECK(!xrce_is_builtin_service("/talker/set_parameters/extra"),
          "the name must be the LAST segment");
    CHECK(!xrce_is_builtin_service("/add_two_ints"), "a user service is not builtin");
    CHECK(!xrce_is_builtin_service("/fib/_action/send_goal"), "an action channel is not builtin");
}

static void claim(void) {
    xrce_session_state_t* st = (xrce_session_state_t*)calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        failures++;
        return;
    }
    printf("        %d server slot(s), %d builtin, request buffer %d / builtin %d\n",
           XRCE_MAX_SERVICE_SERVERS, XRCE_BUILTIN_SERVICE_SERVERS, XRCE_SERVICE_REQUEST_BUFFER_SIZE,
           XRCE_BUILTIN_SERVICE_REQUEST_BUFFER_SIZE);
    if (XRCE_USER_SERVICE_SERVERS > 0) {
        xrce_service_server_slot* s = xrce_claim_server_slot(st, 0, XRCE_USER_SERVICE_SERVERS);
        CHECK(s == &st->service_server_slots[0], "the first free application slot is claimed");
        CHECK(s != NULL && s->ring_bytes == &st->service_request_bytes[0][0][0],
              "bound to the application family's bytes");
        CHECK(s != NULL && s->entry_cap == XRCE_SERVICE_REQUEST_BUFFER_SIZE,
              "at the application family's size");
        if (s != NULL) s->active = true;
        if (XRCE_USER_SERVICE_SERVERS > 1) {
            xrce_service_server_slot* t = xrce_claim_server_slot(st, 0, XRCE_USER_SERVICE_SERVERS);
            CHECK(t == &st->service_server_slots[1] &&
                      t->ring_bytes == &st->service_request_bytes[1][0][0],
                  "an active slot is skipped, and the next is bound to its own bytes");
        }
    }
    CHECK(xrce_claim_server_slot(st, XRCE_USER_SERVICE_SERVERS, XRCE_MAX_SERVICE_SERVERS) == NULL ||
              XRCE_BUILTIN_SERVICE_SERVERS > 0,
          "with no split there is no builtin slot to claim");
    free(st);
}

int main(void) {
    printf("issue 1808: XRCE service-server families\n");
    names();
    claim();
    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
