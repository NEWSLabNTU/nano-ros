/* micro-XRCE-DDS-Client RMW backend — vtable assembly + register entry point.
 *
 * Phase 115.K.2: every slot points at the matching stub function in
 * session.c / publisher.c / subscriber.c / service.c. Stubs return
 * NROS_RMW_RET_UNSUPPORTED so the runtime sees a wired-but-inert
 * backend until 115.K.2.1+ fills them in with `uxr_*` calls.
 */

#include "nros_rmw_xrce.h"

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "internal.h"

/* Phase 108 event hooks left NULL until a follow-up phase wires
 * micro-XRCE-DDS-Client status callbacks through to the runtime's
 * status-event surface. */
static const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    .open                       = xrce_session_open,
    .close                      = xrce_session_close,
    .drive_io                   = xrce_session_drive_io,

    /* ---- Publisher ---- */
    .create_publisher           = xrce_publisher_create,
    .destroy_publisher          = xrce_publisher_destroy,
    .publish_raw                = xrce_publisher_publish_raw,

    /* ---- Subscriber ---- */
    .create_subscriber          = xrce_subscriber_create,
    .destroy_subscriber         = xrce_subscriber_destroy,
    .try_recv_raw               = xrce_subscriber_try_recv_raw,
    .has_data                   = xrce_subscriber_has_data,

    /* ---- Service Server ---- */
    .create_service_server      = xrce_service_server_create,
    .destroy_service_server     = xrce_service_server_destroy,
    .try_recv_request           = xrce_service_try_recv_request,
    .has_request                = xrce_service_has_request,
    .send_reply                 = xrce_service_send_reply,

    /* ---- Service Client ---- */
    .create_service_client      = xrce_service_client_create,
    .destroy_service_client     = xrce_service_client_destroy,
    .call_raw                   = xrce_service_call_raw,

    /* ---- Phase 108 / 110.0 hooks (deferred) ---- */
    .register_subscriber_event  = NULL,
    .register_publisher_event   = NULL,
    .assert_publisher_liveliness = NULL,
    .next_deadline_ms           = NULL,
};

nros_rmw_ret_t nros_rmw_xrce_register(void) {
    return nros_rmw_cffi_register(&kVtable);
}
