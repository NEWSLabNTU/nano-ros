/* Issue 0847 — an entity handle that outlives its session must not fault.
 *
 * THE ORDERING THIS ASSERTS is the one the bench binaries and the docs both
 * show, and it is not exotic:
 *
 *     executor.close();          // xrce_session_destroy -> free(session_state)
 *     ...                        // publisher still alive in the caller's scope
 *     <end of scope>             // xrce_publisher_destroy -> reads freed memory
 *
 * Every `nros-bench/stress-xrce` run dumped core at exit on this host, on both
 * the talker and the listener, AFTER `PUBLISH_DONE` / `RECV_DONE` had printed.
 * That is why it survived: a segfault after the last line of output looks like
 * a clean run to anything that greps stdout, which is what the harness does.
 *
 * WHY A UNIT TEST AND NOT AN E2E ONE. The issue asks for a test that asserts
 * the process EXIT STATUS rather than its output, and that is exactly what this
 * is -- but at a level that needs no XRCE agent. The failure mode is a
 * use-after-free inside a destructor, so BEFORE the fix this file does not
 * report a failure, it DIES: the assertions below are reached only on a build
 * where the entity path no longer dereferences a freed session. ctest reads the
 * exit status, so a SIGSEGV here is a red, which is the whole point.
 *
 * NO AGENT IS NEEDED because a closed session's destructor must not touch the
 * transport at all -- that is the invariant under test. The session state is
 * built directly rather than through `xrce_session_create`, which would open a
 * socket and hand-shake; what matters here is the pointer relationship, not the
 * wire.
 */

#include "internal.h"
#include "nros/rmw_ret.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, what)                                                      \
    do {                                                                       \
        if (!(cond)) {                                                         \
            printf("  FAIL: %s\n", (what));                                    \
            failures++;                                                        \
        } else {                                                               \
            printf("  ok    %s\n", (what));                                    \
        }                                                                      \
    } while (0)

/* A session state with no transport and no agent. `calloc` gives every field
 * the zero the real creator would leave it at before the handshake, which is
 * all the entity paths under test read. */
static xrce_session_state_t *make_session(void) {
    xrce_session_state_t *st =
        (xrce_session_state_t *)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        exit(1);
    }
    return st;
}

/* The crashing order: close the session while a publisher still holds it, then
 * destroy the publisher. */
static void publisher_outlives_session(void) {
    printf("publisher destroyed AFTER its session is closed\n");

    xrce_session_state_t *st = make_session();
    xrce_publisher_state *ps =
        (xrce_publisher_state *)nros_xrce_calloc(1, sizeof(xrce_publisher_state));
    ps->session_state = st;
    xrce_session_entity_attach(st);

    rmw_publisher_t pub;
    memset(&pub, 0, sizeof(pub));
    pub.backend_data = ps;

    /* What `xrce_session_destroy` does to the state, minus the transport
     * teardown this fixture has no transport for. The FREE is the part under
     * test: it must not happen while `ps` still points here. */
    st->session_closed = true;
    CHECK(st->live_entities == 1,
          "the session knows one entity still points at it");

    /* Pre-fix this call read `st->session` out of freed memory and the process
     * died here. */
    rmw_ret_t ret = xrce_publisher_destroy(&pub);
    CHECK(ret == NROS_RMW_RET_OK,
          "destroying an entity after close is OK, not an error -- the ordering "
          "is supported, not a caller mistake");
    CHECK(pub.backend_data == NULL, "the handle is cleared");
    /* `st` is freed by that call, so it must not be read again. Nothing below
     * touches it; that is the contract `xrce_session_entity_detach` documents. */
}

/* An OPEN session must keep its state when an entity goes away: entities come
 * and go while a session runs, and freeing on the first detach would be a far
 * worse bug than the one being fixed. */
static void entity_destroyed_while_session_open(void) {
    printf("entity destroyed while the session is still OPEN\n");

    xrce_session_state_t *st = make_session();
    xrce_publisher_state *ps =
        (xrce_publisher_state *)nros_xrce_calloc(1, sizeof(xrce_publisher_state));
    ps->session_state = st;
    xrce_session_entity_attach(st);

    xrce_session_entity_detach(st);
    /* Reading `st` here is only legal because it must NOT have been freed --
     * which is the assertion. Under ASAN a regression makes this a
     * use-after-free rather than a quiet wrong answer. */
    CHECK(st->live_entities == 0, "the count drops");
    CHECK(st->session_closed == false, "an open session stays open");

    nros_xrce_free(ps);
    nros_xrce_free(st);
}

/* The count has to be right with several entities outstanding, or the free
 * lands early -- which is the original bug with extra steps. */
static void several_entities_outstanding(void) {
    printf("three entities, closed session, freed by the LAST one out\n");

    xrce_session_state_t *st = make_session();
    xrce_session_entity_attach(st);
    xrce_session_entity_attach(st);
    xrce_session_entity_attach(st);
    CHECK(st->live_entities == 3, "three attached");

    st->session_closed = true;
    xrce_session_entity_detach(st);
    /* Still alive: two handles remain. */
    CHECK(st->live_entities == 2, "one out, two remain, state still alive");
    xrce_session_entity_detach(st);
    CHECK(st->live_entities == 1, "two out, one remains");
    /* The third detach frees `st`; nothing may read it afterwards. */
    xrce_session_entity_detach(st);
    printf("  ok    the last detach freed the session state\n");
}

/* A session closed with NOTHING outstanding must free immediately -- the
 * ordinary case, and the one a deferral could silently turn into a leak. */
static void close_with_no_entities_frees_now(void) {
    printf("session closed with no entities outstanding\n");
    xrce_session_state_t *st = make_session();
    CHECK(st->live_entities == 0, "nothing attached");
    CHECK(xrce_session_is_closed(st) == false, "a fresh session is open");
    st->session_closed = true;
    CHECK(xrce_session_is_closed(st) == true, "close is observable to a destructor");
    nros_xrce_free(st);
}

/* THE REPORTED BUG, through the REAL `xrce_session_destroy`.
 *
 * The cases above drive the helpers; this one drives the actual close path, so
 * a regression in `xrce_session_destroy`'s own free is caught rather than
 * simulated. Restoring the pre-fix unconditional `nros_xrce_free(st)` there
 * makes THIS case read freed memory -- which is the whole defect.
 *
 * It drives `xrce_session_mark_closed` rather than `xrce_session_destroy`:
 * MEASURED, the full destroy faults in `uxr_delete_session -> wait_session_status`
 * without a live transport, which is a property of the uxr client and nothing to
 * do with this defect. The lifetime decision was split into its own function so
 * the part that actually owns the free is reachable here. */
static void real_session_destroy_with_live_publisher(void) {
    printf("REAL xrce_session_destroy with a publisher still alive\n");

    xrce_session_state_t *st = make_session();
    xrce_publisher_state *ps =
        (xrce_publisher_state *)nros_xrce_calloc(1, sizeof(xrce_publisher_state));
    ps->session_state = st;
    xrce_session_entity_attach(st);

    rmw_publisher_t pub;
    memset(&pub, 0, sizeof(pub));
    pub.backend_data = ps;

    xrce_session_mark_closed(st);
    CHECK(xrce_session_is_closed(st) == true, "the session reports closed");

    /* Pre-fix, `st` is freed by now and the next line is the use-after-free
     * that dumped core in every stress-xrce run. */
    rmw_ret_t pret = xrce_publisher_destroy(&pub);
    CHECK(pret == NROS_RMW_RET_OK, "the publisher tears down cleanly afterwards");
    CHECK(pub.backend_data == NULL, "the publisher handle is cleared");
}

int main(void) {
    printf("xrce entity/session lifetime (issue 0847)\n");
    close_with_no_entities_frees_now();
    entity_destroyed_while_session_open();
    several_entities_outstanding();
    publisher_outlives_session();
    real_session_destroy_with_live_publisher();

    if (failures != 0) {
        printf("FAILED: %d assertion(s)\n", failures);
        return 1;
    }
    printf("PASS: entity handles survive their session\n");
    return 0;
}
