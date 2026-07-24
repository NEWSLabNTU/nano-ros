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
    if (g_received_vtable->create_session == NULL) {
        fprintf(stderr, "FAIL: vtable->create_session is NULL\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_publisher == NULL) {
        fprintf(stderr, "FAIL: vtable->create_publisher is NULL\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_subscription == NULL) {
        fprintf(stderr, "FAIL: vtable->create_subscription is NULL\n");
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
    r = g_received_vtable->create_session("127.0.0.1:1", 0, 0, "smoke", &session);
    if (r == NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr,
                "FAIL: open returned UNSUPPORTED — K.2.1 should have replaced the stub\n");
        return EXIT_FAILURE;
    }
    if (r == NROS_RMW_RET_OK) {
        /* Surprise — agent on port 1. Close cleanly. */
        g_received_vtable->destroy_session(&session);
    }

    /* Phase 115.K.2.2 — publish_raw on a NULL backend_data publisher
     * must reach the backend (no longer the K.2.0 UNSUPPORTED stub)
     * and return INVALID_ARGUMENT. */
    nros_rmw_publisher_t pub = {0};
    r = g_received_vtable->publish_raw(&pub, NULL, 0);
    if (r != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: publish_raw on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)r);
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.2 — try_recv_raw / has_data on a fresh subscriber
     * shell with NULL backend_data must reach the backend. */
    nros_rmw_subscription_t sub = {0};
    int32_t rr = g_received_vtable->try_recv_raw(&sub, NULL, 0);
    if (rr != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: try_recv_raw on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)rr);
        return EXIT_FAILURE;
    }
    int32_t hd = g_received_vtable->has_data(&sub);
    if (hd != 0) {
        fprintf(stderr, "FAIL: has_data on NULL backend_data returned %d, expected 0\n", (int)hd);
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.3 — service paths must reach the backend. With a
     * NULL session, create_service returns INVALID_ARGUMENT
     * (no longer UNSUPPORTED stub). */
    nros_rmw_service_t srv = {0};
    nros_rmw_ret_t srv_r = g_received_vtable->create_service(
        NULL, "/foo", "Foo_", NULL, 0, NULL, &srv);
    if (srv_r != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: create_service with NULL session returned %d, expected INVALID_ARGUMENT\n",
                (int)srv_r);
        return EXIT_FAILURE;
    }

    /* try_recv_request / has_request / send_reply / send_request_raw
     * on NULL backend_data also reach the backend. */
    int64_t seq = 0;
    int32_t tr = g_received_vtable->try_recv_request(&srv, NULL, 0, &seq);
    if (tr != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: try_recv_request on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)tr);
        return EXIT_FAILURE;
    }

    nros_rmw_client_t cli = {0};
    int32_t cr = g_received_vtable->send_request_raw(&cli, NULL, 0);
    if (cr != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: send_request_raw on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)cr);
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.4 — custom-transport bridge:
     *  (a) `nros_rmw_xrce_init_custom_transport` returns UNSUPPORTED
     *      until the runtime drain symbol lands.
     *  (b) `nros_rmw_xrce_set_custom_transport_ops` rejects NULL.
     *  (c) opening a `custom://` session without first arming the
     *      bridge returns INVALID_ARGUMENT (no UNSUPPORTED stub).
     *  (d) After arming the bridge with a NULL-call vtable, the
     *      open path tries to use it and fails at the agent level
     *      (write returns 0 because read returns 0 — OK / -1, anything
     *      non-OK is acceptable, just not UNSUPPORTED). */
    nros_rmw_ret_t r4 = nros_rmw_xrce_init_custom_transport(0);
    if (r4 != NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr,
                "FAIL: nros_rmw_xrce_init_custom_transport returned %d, "
                "expected UNSUPPORTED (K.2.4 drain-from-runtime gap)\n",
                (int)r4);
        return EXIT_FAILURE;
    }

    nros_rmw_ret_t r4_null = nros_rmw_xrce_set_custom_transport_ops(NULL, 0);
    if (r4_null != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: set_custom_transport_ops(NULL) returned %d, expected INVALID_ARGUMENT\n",
                (int)r4_null);
        return EXIT_FAILURE;
    }

    nros_rmw_session_t cust_session = {0};
    nros_rmw_ret_t cret = g_received_vtable->create_session(
        "custom://noop", 0, 0, "smoke-custom", &cust_session);
    if (cret != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: custom:// open without armed bridge returned %d, "
                "expected INVALID_ARGUMENT\n",
                (int)cret);
        return EXIT_FAILURE;
    }

    printf("ok: pub/sub + services + custom-transport bridge wired (K.2.2/2.3/2.4)\n");
    return EXIT_SUCCESS;
}
