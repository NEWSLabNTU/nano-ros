/* Phase 115.K.2 smoke test.
 *
 * Confirms:
 *   1. The static library compiles + links.
 *   2. `nros_rmw_xrce_register()` reaches its `nros_rmw_cffi_register`
 *      hand-off and propagates the return code unchanged.
 *
 * The real `nros_rmw_cffi_register` symbol lives in the
 * `nros-rmw-cffi` Rust crate; this test stubs it with a local
 * implementation that records the vtable pointer it received and
 * returns OK. Validating wire-up at the link layer + sanity-checking
 * that the vtable is non-NULL on the way through.
 */

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"

#include <stdio.h>
#include <stdlib.h>

static const nros_rmw_vtable_t *g_received_vtable = NULL;

nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    g_received_vtable = vtable;
    return NROS_RMW_RET_OK;
}

int main(void) {
    g_received_vtable = NULL;

    nros_rmw_ret_t r = nros_rmw_xrce_register();
    if (r != NROS_RMW_RET_OK) {
        fprintf(stderr, "FAIL: nros_rmw_xrce_register returned %d, expected NROS_RMW_RET_OK\n",
                (int)r);
        return EXIT_FAILURE;
    }
    if (g_received_vtable == NULL) {
        fprintf(stderr, "FAIL: nros_rmw_cffi_register received NULL vtable\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->open == NULL) {
        fprintf(stderr, "FAIL: vtable->open is NULL\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_publisher == NULL) {
        fprintf(stderr, "FAIL: vtable->create_publisher is NULL\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_subscriber == NULL) {
        fprintf(stderr, "FAIL: vtable->create_subscriber is NULL\n");
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.1 — open() now actually attempts UDP transport
     * + uxr_create_session against the configured agent. Without an
     * agent listening, the call fails with NROS_RMW_RET_ERROR; with
     * an agent it returns OK. Either is fine for this smoke; what
     * we care about is that the call REACHES the backend instead
     * of hitting the K.2.0 UNSUPPORTED stub.
     *
     * Use port 1 to make the "no agent" case deterministic — it's
     * reserved + nothing is listening. */
    nros_rmw_session_t session = {0};
    r = g_received_vtable->open("127.0.0.1:1", 0, 0, "smoke", &session);
    if (r == NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr,
                "FAIL: open returned UNSUPPORTED — K.2.1 should have replaced the stub\n");
        return EXIT_FAILURE;
    }
    if (r == NROS_RMW_RET_OK) {
        /* Surprise — agent on port 1. Close cleanly. */
        g_received_vtable->close(&session);
    }

    /* The publish_raw / try_recv_raw / service paths still hit the
     * K.2.0 stubs; confirm at least one. */
    nros_rmw_publisher_t pub = {0};
    r = g_received_vtable->publish_raw(&pub, NULL, 0);
    if (r != NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr,
                "FAIL: publish_raw returned %d, expected UNSUPPORTED (K.2.2 not landed yet)\n",
                (int)r);
        return EXIT_FAILURE;
    }

    printf("ok: vtable wired; session.open reaches backend; pub/sub stubs still UNSUPPORTED\n");
    return EXIT_SUCCESS;
}
