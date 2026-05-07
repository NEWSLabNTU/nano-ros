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

    /* Confirm a stub returns UNSUPPORTED. */
    nros_rmw_session_t session = {0};
    r = g_received_vtable->open("ipv4://127.0.0.1:7400", 0, 0, "smoke", &session);
    if (r != NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr, "FAIL: open stub returned %d, expected NROS_RMW_RET_UNSUPPORTED\n",
                (int)r);
        return EXIT_FAILURE;
    }

    printf("ok: vtable wired, register passes pointer through, stubs return UNSUPPORTED\n");
    return EXIT_SUCCESS;
}
