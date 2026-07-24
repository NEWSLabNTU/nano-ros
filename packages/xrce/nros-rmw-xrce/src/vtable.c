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
    .create_session             = xrce_session_create,
    .destroy_session            = xrce_session_destroy,
    .drive_io                   = xrce_session_drive_io,

    /* ---- Publisher ---- */
    .create_publisher           = xrce_publisher_create,
    .destroy_publisher          = xrce_publisher_destroy,
    .publish_raw                = xrce_publisher_publish_raw,

    /* ---- Subscription ---- */
    .create_subscription        = xrce_subscription_create,
    .destroy_subscription       = xrce_subscription_destroy,
    .try_recv_raw               = xrce_subscription_try_recv_raw,
    .has_data                   = xrce_subscription_has_data,

    /* ---- Service ---- */
    .create_service             = xrce_service_create,
    .destroy_service            = xrce_service_destroy,
    .try_recv_request           = xrce_service_try_recv_request,
    .has_request                = xrce_service_has_request,
    .send_reply                 = xrce_service_send_reply,

    /* ---- Client ---- */
    .create_client              = xrce_client_create,
    .destroy_client             = xrce_client_destroy,

    /* Phase 130.4 — non-blocking send/recv split. Lets the
     * executor's spin loop poll for a late-arriving reply without
     * re-sending the request (Phase 127.C.4 C++ action send_goal
     * root cause). Phase-301 deleted the deprecated blocking
     * call_raw slot; this pair is the one request/reply path. */
    .send_request_raw           = xrce_service_send_request_raw,
    .try_recv_reply_raw         = xrce_service_try_recv_reply_raw,

    /* ---- Phase 108 / 110.0 / 104.C.6.b hooks (deferred) ---- */
    .register_subscription_event = NULL,
    .register_publisher_event   = NULL,
    .assert_publisher_liveliness = NULL,
    .next_deadline_ms           = NULL,
    /* Phase 124.B.1 — XRCE has no asynchronous notify path
     * (XRCE-DDS-Client is poll-driven via xrce_session_drive_io).
     * NULL = runtime drains this backend on its deadline-bound
     * cv-wait boundary; same-thread setters (Executor::wake, halt)
     * still drive the wake_cv. */
    .set_wake_callback          = NULL,

    /* Phase 124.A — zero-copy ABI. XRCE-DDS-Client uses micro-CDR
     * with caller-provided staging buffers; loan/borrow would
     * require a per-publisher arena equivalent. Leave NULL; runtime
     * falls back to the staging-buffer path. */
    .pub_loan                   = NULL,
    .pub_commit                 = NULL,
    .pub_discard                = NULL,
    .sub_borrow                 = NULL,
    .sub_release                = NULL,

    /* Phase 124.C — service availability probe. micro-XRCE-DDS-Client
     * has no participant enumeration; leave NULL → runtime surfaces
     * NROS_RMW_RET_UNSUPPORTED. */
    .service_server_available   = NULL,

    /* Phase 124.D — native batch take. XRCE delivers one sample per
     * topic callback into a single-slot inbox; no native take_n.
     * Leave NULL → runtime emits the try_recv_raw loop fallback. */
    .try_recv_sequence          = NULL,

    /* Phase 124.E.3 — streamed publish via uxr_prepare_output_stream
     * (writes the payload straight into the reliable output stream,
     * no per-publisher staging buffer). */
    .publish_streamed           = xrce_publisher_publish_streamed,

    /* Phase 124.F.2 — connectivity probe via uxr_ping_agent_session. */
    .ping_session               = xrce_session_ping,

    /* Phase 231 (RFC-0038) — zero-copy in-place take over the XRCE static
     * ring (the staged `entry->data`), so the executor's in-place dispatch
     * borrows the bytes instead of copying into an arena buffer. */
    .subscription_supports_in_place = xrce_subscription_supports_in_place,
    .process_raw_in_place           = xrce_subscription_process_raw_in_place,
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
