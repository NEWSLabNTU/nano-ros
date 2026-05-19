// Cyclone DDS RMW backend — vtable assembly + register entry point.
//
// Phase 117.3: every slot points at the matching stub function in
// session.cpp / publisher.cpp / subscriber.cpp / service.cpp. Stubs
// return NROS_RMW_RET_UNSUPPORTED so the runtime sees a wired-but-
// inert backend until 117.4–117.7 fill them in.

#include "nros_rmw_cyclonedds.h"

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "internal.hpp"

namespace {

using namespace nros_rmw_cyclonedds;

// Phase 108 event hooks left NULL until a follow-up phase wires
// Cyclone listeners through to the runtime's status-event surface.
constexpr nros_rmw_ret_t (*kRegisterSubscriberEvent)(
    nros_rmw_subscriber_t *, nros_rmw_event_kind_t, uint32_t,
    nros_rmw_event_callback_t, void *) = nullptr;
constexpr nros_rmw_ret_t (*kRegisterPublisherEvent)(
    nros_rmw_publisher_t *, nros_rmw_event_kind_t, uint32_t,
    nros_rmw_event_callback_t, void *) = nullptr;
constexpr nros_rmw_ret_t (*kAssertPublisherLiveliness)(
    nros_rmw_publisher_t *) = nullptr;

const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    /*open*/                      session_open,
    /*close*/                     session_close,
    /*drive_io*/                  session_drive_io,

    /* ---- Publisher ---- */
    /*create_publisher*/          publisher_create,
    /*destroy_publisher*/         publisher_destroy,
    /*publish_raw*/               publisher_publish_raw,

    /* ---- Subscriber ---- */
    /*create_subscriber*/         subscriber_create,
    /*destroy_subscriber*/        subscriber_destroy,
    /*try_recv_raw*/              subscriber_try_recv_raw,
    /*has_data*/                  subscriber_has_data,

    /* ---- Service Server ---- */
    /*create_service_server*/     service_server_create,
    /*destroy_service_server*/    service_server_destroy,
    /*try_recv_request*/          service_try_recv_request,
    /*has_request*/               service_has_request,
    /*send_reply*/                service_send_reply,

    /* ---- Service Client ---- */
    /*create_service_client*/     service_client_create,
    /*destroy_service_client*/    service_client_destroy,
    /*call_raw*/                  service_call_raw,
    /* Phase 130.8 — non-blocking send/recv split. Skips the
     * CFFI legacy blocking-call_raw fallback so the executor's
     * spin loop polls for replies without re-sending the
     * request. */
    /*send_request_raw*/          service_send_request_raw,
    /*try_recv_reply_raw*/        service_try_recv_reply_raw,

    /* ---- Phase 108 event hooks (deferred) ---- */
    /*register_subscriber_event*/ kRegisterSubscriberEvent,
    /*register_publisher_event*/  kRegisterPublisherEvent,
    /*assert_publisher_liveliness*/ kAssertPublisherLiveliness,
    /* ---- Phase 110.0 + 104.C.6.b hooks (deferred) ---- */
    /*next_deadline_ms*/          nullptr,
    /* Phase 124.B.1 — Cyclone DDS has its own background threads
     * for sample arrival + matched-entity events; wiring the wake
     * callback into those listeners is a follow-up (lives in the
     * listener-installation path, not this static vtable). nullptr
     * today; runtime drains on deadline-bound cv-wait boundary. */
    /*set_wake_callback*/         nullptr,

    /* Phase 124.A — zero-copy ABI. Cyclone DDS supports loan via
     * dds_loan_sample / dds_return_loan; wire-up is a follow-up
     * (track under 124.A.5). nullptr today → runtime falls back to
     * the arena staging-buffer path on this backend. */
    /*pub_loan*/                  nullptr,
    /*pub_commit*/                nullptr,
    /*pub_discard*/               nullptr,
    /*sub_borrow*/                nullptr,
    /*sub_release*/               nullptr,

    /* Phase 124.C — service availability probe. Deferred until the
     * Cyclone DDS built-in topic readers are wired through (matches
     * the 124.C.2 DDS blocker). nullptr → runtime surfaces
     * NROS_RMW_RET_UNSUPPORTED, no stub. */
    /*service_server_available*/  nullptr,

    /* Phase 124.D.3 — native batch take. Cyclone provides
     * `dds_take(reader, buf, info, count, maxs)` as a single-call
     * batch API; we wrap it in subscriber_try_recv_sequence with
     * CDR re-serialisation per slot. */
    /*try_recv_sequence*/         subscriber_try_recv_sequence,

    /* Phase 124.E — continuous serialization. nullptr → runtime
     * staging-buffer fallback. */
    /*publish_streamed*/          nullptr,
};

} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cyclonedds_register(void) {
    return nros_rmw_cffi_register_named("cyclonedds", &kVtable);
}

// Phase 128.B.4 — `.nros_rmw_init` self-registration via the canonical
// macro from <nros/rmw_vtable.h>. The runtime walker
// (`nros_rmw_cffi_walk_init_section`) discovers this entry on first
// `nros::init` and calls `nros_rmw_cyclonedds_register` — the C/C++
// side gets full nameless dispatch (no `#ifdef NROS_RMW_CYCLONEDDS`
// chain anywhere). Static-lib link with `--whole-archive` ensures the
// section entry survives stripping.
extern "C" {
static void nros_rmw_cyclonedds_section_register(void) {
    (void) nros_rmw_cyclonedds_register();
}
}
NROS_RMW_REGISTER_BACKEND(nros_rmw_cyclonedds_section_register)
