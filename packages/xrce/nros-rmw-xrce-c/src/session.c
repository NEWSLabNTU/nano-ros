/* Session lifecycle stubs.
 *
 * Phase 115.K.2 scaffold — every entry returns NROS_RMW_RET_UNSUPPORTED
 * so the runtime sees the backend as wired-but-inert. Replaced in
 * 115.K.2.1+ by `uxr_session_*` calls that mirror the existing Rust
 * `nros-rmw-xrce` impl.
 */

#include "internal.h"

#include "nros/rmw_ret.h"

nros_rmw_ret_t xrce_session_open(const char *locator, uint8_t mode,
                                 uint32_t domain_id, const char *node_name,
                                 nros_rmw_session_t *out) {
    (void)locator;
    (void)mode;
    (void)domain_id;
    (void)node_name;
    (void)out;
    return NROS_RMW_RET_UNSUPPORTED;
}

nros_rmw_ret_t xrce_session_close(nros_rmw_session_t *session) {
    (void)session;
    return NROS_RMW_RET_UNSUPPORTED;
}

nros_rmw_ret_t xrce_session_drive_io(nros_rmw_session_t *session,
                                     int32_t timeout_ms) {
    (void)session;
    (void)timeout_ms;
    return NROS_RMW_RET_UNSUPPORTED;
}
