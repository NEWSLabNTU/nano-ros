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

    /* ---- Phase 108 / 110.0 / 104.C.6.b hooks (deferred) ---- */
    .register_subscriber_event  = NULL,
    .register_publisher_event   = NULL,
    .assert_publisher_liveliness = NULL,
    .next_deadline_ms           = NULL,
    /* Phase 104.C.6.b — the XRCE backend has no asynchronous notify
     * path that could write into the executor's shared wake flag
     * (XRCE-DDS-Client is poll-driven via xrce_session_drive_io).
     * NULL = runtime treats this backend as "purely poll-based" and
     * relies on cooperative scheduling + the same-thread setters
     * (Executor::wake, halt, …). */
    .set_wake_signal            = NULL,

    /* Phase 124.A — zero-copy ABI. XRCE-DDS-Client uses micro-CDR
     * with caller-provided staging buffers; loan/borrow would
     * require a per-publisher arena equivalent. Leave NULL; runtime
     * falls back to the staging-buffer path. */
    .pub_loan                   = NULL,
    .pub_commit                 = NULL,
    .pub_discard                = NULL,
    .sub_borrow                 = NULL,
    .sub_release                = NULL,
};

nros_rmw_ret_t nros_rmw_xrce_register(void) {
    /* Phase 104.B.2 — register under the canonical name "xrce" so
     * bridge code (and `Executor::create_node_with_rmw("name", "xrce",
     * ...)`) can resolve this backend through the named registry. */
    return nros_rmw_cffi_register_named("xrce", &kVtable);
}

/* Phase 115.K.2.5.2 — auto-register on library load.
 *
 * The C/C++ APIs go through `nros_support_init` (C) or `nros::init`
 * (C++). The C++ path explicitly calls `nros_rmw_xrce_register`
 * inside `nros::init` (gated on `NROS_RMW_XRCE` from CMake). The
 * pure-C path doesn't have an analogous explicit hook today, so we
 * piggy-back on the loader's GCC/Clang `__attribute__((constructor))`
 * hook. The constructor runs before `main()` (and on RTOS targets
 * before `app_main`) on every toolchain we currently target
 * (gcc/clang on glibc, musl, newlib + the rust-staticlib link path).
 *
 * MSVC users (out-of-scope today) would need a `#pragma section`
 * `.CRT$XCU` shim instead. The C++ explicit-call path remains the
 * portable fallback if a target lacks ELF/COFF constructor support.
 */
#if defined(__GNUC__) || defined(__clang__)
__attribute__((constructor))
static void nros_rmw_xrce_register_ctor(void) {
    (void)nros_rmw_cffi_register_named("xrce", &kVtable);
}
#endif
