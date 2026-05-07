/* Phase 115.K.2.4 — runtime custom-transport bridge.
 *
 * This file is a placeholder until 115.K.2.4 lands. It exposes the
 * stubs the rest of the backend links against, so 115.K.2.2 / 2.3
 * can build cleanly. The real implementation arrives in 115.K.2.4.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

int xrce_custom_transport_is_armed(void) {
    return 0;
}

nros_rmw_ret_t xrce_custom_transport_install(xrce_session_state_t *st,
                                             bool framing) {
    (void)st;
    (void)framing;
    /* Phase 115.K.2.2 / 2.3 — `custom://` locator selected without
     * the K.2.4 bridge available. */
    return NROS_RMW_RET_UNSUPPORTED;
}
