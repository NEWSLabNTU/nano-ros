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
constexpr nros_rmw_ret_t (*kRegisterSubscriptionEvent)(
    nros_rmw_subscription_t *, nros_rmw_event_kind_t, uint32_t,
    nros_rmw_event_callback_t, void *) = nullptr;
constexpr nros_rmw_ret_t (*kRegisterPublisherEvent)(
    nros_rmw_publisher_t *, nros_rmw_event_kind_t, uint32_t,
    nros_rmw_event_callback_t, void *) = nullptr;
constexpr nros_rmw_ret_t (*kAssertPublisherLiveliness)(
    nros_rmw_publisher_t *) = nullptr;

const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    /*create_session*/            session_create,
    /*destroy_session*/           session_destroy,
    /*drive_io*/                  session_drive_io,

    /* ---- Publisher ---- */
    /*create_publisher*/          publisher_create,
    /*destroy_publisher*/         publisher_destroy,
    /*publish_raw*/               publisher_publish_raw,

    /* ---- Subscription ---- */
    /*create_subscription*/       subscription_create,
    /*destroy_subscription*/      subscription_destroy,
    /*try_recv_raw*/              subscription_try_recv_raw,
    /*has_data*/                  subscription_has_data,

    /* ---- Service ---- */
    /*create_service*/            service_create,
    /*destroy_service*/           service_destroy,
    /*try_recv_request*/          service_try_recv_request,
    /*has_request*/               service_has_request,
    /*send_reply*/                service_send_reply,

    /* ---- Client ---- */
    /*create_client*/             client_create,
    /*destroy_client*/            client_destroy,
    /* Phase 130.8 — non-blocking send/recv split; phase-301 deleted
     * the deprecated blocking call_raw slot, so this pair is the one
     * request/reply path. */
    /*send_request_raw*/          service_send_request_raw,
    /*try_recv_reply_raw*/        service_try_recv_reply_raw,

    /* ---- Phase 108 event hooks (deferred) ---- */
    /*register_subscription_event*/ kRegisterSubscriptionEvent,
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
     * batch API; we wrap it in subscription_try_recv_sequence with
     * CDR re-serialisation per slot. */
    /*try_recv_sequence*/         subscription_try_recv_sequence,

    /* Phase 124.E — continuous serialization. nullptr → runtime
     * staging-buffer fallback. */
    /*publish_streamed*/          nullptr,

    /* Phase 124.F — connectivity probe. No participant ping on
     * Cyclone; nullptr → runtime surfaces UNSUPPORTED. */
    /*ping_session*/              nullptr,

    /* Phase 231 (RFC-0038) — in-place take. Not wired on this
     * backend; nullptr → runtime uses the buffered path. */
    /*subscription_supports_in_place*/ nullptr,
    /*process_raw_in_place*/      nullptr,
};

} // namespace

#ifdef __ZEPHYR__
// Phase 11W.6 — route Cyclone DDS log messages to Zephyr's LOG
// subsystem so init-time fatal errors surface in `west build -t run`
// output. Default sink calls `fwrite(..., stderr)` which picolibc
// silently drops on native_sim; result is a bare `abort()` with no
// diagnostic. Installing a sink that hands the message to Zephyr's
// printk gives us readable failure messages.
// Phase 180.A — do NOT wrap <zephyr/logging/log.h> in extern "C": it is
// C++-safe (self-guards its own C symbols), and on Zephyr 4.x cbprintf.h
// pulls cbprintf_cxx.h (overloaded z_cbprintf_cxx_is_pchar) which a
// surrounding extern "C" turns into conflicting C functions. The manual
// wrap was harmless on 3.7 but fatal on 4.4.
#include <zephyr/logging/log.h>
LOG_MODULE_REGISTER(cyclonedds, LOG_LEVEL_INF);

#include <dds/ddsrt/log.h>

namespace {
void zephyr_log_sink(void *userdata, const dds_log_data_t *data) {
    (void)userdata;
    if (data == nullptr || data->message == nullptr) {
        return;
    }
    // `data->size` excludes the trailing NUL; Cyclone guarantees a
    // NUL is present at `message[size]`.
    LOG_INF("cyclone: %.*s", static_cast<int>(data->size), data->message);
}
} // namespace
#endif

extern "C" __attribute__((weak)) void nros_rmw_cyclonedds_register_app_descriptors(void) {}

extern "C" nros_rmw_ret_t nros_rmw_cyclonedds_register(void) {
    nros_rmw_cyclonedds_register_app_descriptors();
#ifdef __ZEPHYR__
    dds_set_log_sink(zephyr_log_sink, nullptr);
    dds_set_trace_sink(zephyr_log_sink, nullptr);
    dds_set_log_mask(DDS_LC_ALL);

    // Phase 11W.8 — direct NSOS bind probe (placed inline; needs the
    // Zephyr socket symbols already extern-Cd via zephyr_ipv4_compat.h
    // / picolibc autoconf). Mirrors Cyclone's bind setup: AF_INET
    // UDP socket bound to 127.0.0.1:0.
    // Phase 11W.8 probe (removed) — confirmed direct zsock_bind on
    // Zephyr NSOS rejects 127.0.0.1 with errno=2 (ENOENT) but accepts
    // 0.0.0.0. Cyclone's `ddsi_ownip` rejects 0.0.0.0 as the
    // participant's advertised address. Resolution belongs to a
    // follow-up phase — either patch NSOS, or coerce Cyclone to bind
    // to 0.0.0.0 with an explicit `<NetworkInterface>` config that
    // advertises a routable address while letting the socket bind to
    // ANY.
#endif
    // Phase 169.5 — Cyclone is the sole DDS backend, registered
    // under its canonical name "cyclonedds" ONLY. Callers select via
    // `NROS_RMW=cyclonedds`; the generic
    // `"dds"` slot is not aliased per user direction (always
    // reference Cyclone by its specific name, not the generic one).
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

#ifndef __ZEPHYR__
// `.init_array` self-registration for the native / hosted C and C++
// API path. The section walker above only fires when `nros-rmw-cffi`
// is built with `linkme-register` ON, but `nros-node` pulls it with
// `default-features = false` and its `rmw-cffi` feature does not
// re-enable `linkme-register`, so on the C-API path the walker is the
// no-op stub (returns 0) and the linkme entry is never invoked —
// `nros_support_init` then comes up with an empty registry and returns
// `NROS_RET_INVALID_ARGUMENT` (-3). A constructor runs before `main()`
// (hence before `nros_support_init`) regardless of the walker. The
// `--whole-archive` link keeps this object's `.init_array` slot.
// `nros_rmw_cffi_register_named` is idempotent (same-name overwrite),
// so this is harmless when the walker IS active (Rust-API builds).
//
// Gated off Zephyr: there `.init_array` constructors are not run by the
// startup path, and registration is wired explicitly via
// `nros_cpp_init` / `nros_app_register_backends` instead.
__attribute__((constructor)) static void nros_rmw_cyclonedds_ctor_register(void) {
    (void) nros_rmw_cyclonedds_register();
}
#endif
