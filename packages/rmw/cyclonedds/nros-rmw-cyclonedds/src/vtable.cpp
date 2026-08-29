// Cyclone DDS RMW backend — vtable assembly + register entry point.
//
// Phase 117.3: every slot points at the matching stub function in
// session.cpp / publisher.cpp / subscriber.cpp / service.cpp. Stubs
// return NROS_RMW_RET_UNSUPPORTED so the runtime sees a wired-but-
// inert backend until 117.4–117.7 fill them in.

#include <cstring>

#include "nros_rmw_cyclonedds.h"

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "internal.hpp"

namespace {

/* issue 0800 — `set_log_severity` had a slot, a runtime dispatcher and stub
 * tests since phase-376 W5, and no backend body: every image answered
 * UNSUPPORTED while Cyclone has had `dds_set_log_mask` all along. That is the
 * shape 0800 is about — a declared capability nothing implements reading as
 * covered.
 *
 * Cyclone's control is a CATEGORY BITMASK, not a level ladder, so the ladder is
 * mapped onto cumulative masks: each severity enables itself and everything
 * more urgent. DEBUG opens `DDS_LC_ALL` because every category other than
 * FATAL/ERROR/WARNING/INFO falls into trace (`ddsrt/log.h`), which is what a
 * caller asking for DEBUG wants.
 *
 * UNSET is refused rather than guessed: it means "no severity stated", and a
 * backend inventing one would be choosing a verbosity the caller did not ask
 * for. */
/* phase-393 W2 — the three cheap reads, each backed by a Cyclone primitive.
 *
 * `dds_get_matched_*` with a NULL array and 0 capacity returns the COUNT: the
 * API rejects `rds == NULL && nrds > 0`, so asking for none is how you ask how
 * many. That answers "is anyone subscribed to this topic", which is the
 * question an operator asks when nothing arrives — and until now the runtime
 * could not answer it in any language. */
static rmw_ret_t cyclone_publisher_count_matched_subscriptions(const rmw_publisher_t *publisher,
                                                               size_t *subscription_count) {
    if (publisher == nullptr || subscription_count == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    dds_return_t n =
        dds_get_matched_subscriptions(nros_rmw_cyclonedds::publisher_writer(publisher), nullptr, 0);
    if (n < 0) {
        return NROS_RMW_RET_ERROR;
    }
    *subscription_count = (size_t)n;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t cyclone_subscription_count_matched_publishers(
    const rmw_subscription_t *subscription, size_t *publisher_count) {
    if (subscription == nullptr || publisher_count == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    dds_return_t n = dds_get_matched_publications(
        nros_rmw_cyclonedds::subscription_reader(subscription), nullptr, 0);
    if (n < 0) {
        return NROS_RMW_RET_ERROR;
    }
    *publisher_count = (size_t)n;
    return NROS_RMW_RET_OK;
}

/* Cyclone's GUID is 16 bytes; `rmw_gid_t::data` is 24 (upstream's
 * `RMW_GID_STORAGE_SIZE`). Zero-pad the tail rather than leaving it
 * uninitialised — a gid is COMPARED, and comparing 8 bytes of stack residue
 * makes two reads of the same publisher differ. */
static rmw_ret_t cyclone_get_gid_for_publisher(const rmw_publisher_t *publisher, rmw_gid_t *gid) {
    if (publisher == nullptr || gid == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    dds_guid_t guid;
    std::memset(&guid, 0, sizeof(guid));
    if (dds_get_guid(nros_rmw_cyclonedds::publisher_writer(publisher), &guid) != DDS_RETCODE_OK) {
        return NROS_RMW_RET_ERROR;
    }
    std::memset(gid->data, 0, RMW_GID_STORAGE_SIZE);
    static_assert(sizeof(guid.v) <= RMW_GID_STORAGE_SIZE, "cyclone GUID must fit rmw_gid_t");
    std::memcpy(gid->data, guid.v, sizeof(guid.v));
    /* The identifier is what makes two gids comparable at all (rmw_entity.h);
     * a gid without it can be compared against a foreign backend's by mistake. */
    gid->implementation_identifier = "cyclonedds";
    return NROS_RMW_RET_OK;
}

/* phase-393 W1 — the client/service half of the QoS read-back (issue 0823).
 *
 * Same `out`-carries-the-request contract as the publisher and subscription
 * forms above. The entity each one names is not symmetric between the two
 * sides; `internal.hpp` spells the mapping out. */
static rmw_ret_t cyclone_client_request_publisher_get_actual_qos(const rmw_client_t *client,
                                                                 rmw_qos_profile_t *qos) {
    if (client == nullptr || qos == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return nros_rmw_cyclonedds::read_entity_qos(
        nros_rmw_cyclonedds::client_request_writer(client), qos);
}

static rmw_ret_t cyclone_client_response_subscription_get_actual_qos(const rmw_client_t *client,
                                                                     rmw_qos_profile_t *qos) {
    if (client == nullptr || qos == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return nros_rmw_cyclonedds::read_entity_qos(
        nros_rmw_cyclonedds::client_response_reader(client), qos);
}

static rmw_ret_t cyclone_service_request_subscription_get_actual_qos(const rmw_service_t *service,
                                                                     rmw_qos_profile_t *qos) {
    if (service == nullptr || qos == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return nros_rmw_cyclonedds::read_entity_qos(
        nros_rmw_cyclonedds::service_request_reader(service), qos);
}

static rmw_ret_t cyclone_service_response_publisher_get_actual_qos(const rmw_service_t *service,
                                                                   rmw_qos_profile_t *qos) {
    if (service == nullptr || qos == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return nros_rmw_cyclonedds::read_entity_qos(
        nros_rmw_cyclonedds::service_response_writer(service), qos);
}

/* issue 0823 — QoS is a negotiation; report what was GRANTED.
 *
 * `out` is pre-loaded with the requested profile so a field Cyclone does not
 * report stays as requested rather than reading back as zero. The runtime's
 * caller can then compare the two: equality is silence, a difference is the
 * diagnostic. */
static rmw_ret_t cyclone_publisher_get_actual_qos(const rmw_publisher_t *publisher,
                                                  rmw_qos_profile_t *qos) {
    if (publisher == nullptr || qos == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return nros_rmw_cyclonedds::read_entity_qos(
        nros_rmw_cyclonedds::publisher_writer(publisher), qos);
}

static rmw_ret_t cyclone_subscription_get_actual_qos(const rmw_subscription_t *subscription,
                                                     rmw_qos_profile_t *qos) {
    if (subscription == nullptr || qos == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    return nros_rmw_cyclonedds::read_entity_qos(
        nros_rmw_cyclonedds::subscription_reader(subscription), qos);
}

static rmw_ret_t cyclone_set_log_severity(rmw_log_severity_t severity) {
    uint32_t mask;
    switch (severity) {
    case RMW_LOG_SEVERITY_FATAL:
        mask = DDS_LC_FATAL;
        break;
    case RMW_LOG_SEVERITY_ERROR:
        mask = DDS_LC_FATAL | DDS_LC_ERROR;
        break;
    case RMW_LOG_SEVERITY_WARN:
        mask = DDS_LC_FATAL | DDS_LC_ERROR | DDS_LC_WARNING;
        break;
    case RMW_LOG_SEVERITY_INFO:
        mask = DDS_LC_FATAL | DDS_LC_ERROR | DDS_LC_WARNING | DDS_LC_INFO;
        break;
    case RMW_LOG_SEVERITY_DEBUG:
        mask = DDS_LC_ALL;
        break;
    case RMW_LOG_SEVERITY_UNSET:
    default:
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    dds_set_log_mask(mask);
    return NROS_RMW_RET_OK;
}


using namespace nros_rmw_cyclonedds;

// Phase 108 event hooks left NULL until a follow-up phase wires
// Cyclone listeners through to the runtime's status-event surface.
constexpr rmw_ret_t (*kRegisterSubscriptionEvent)(
    const rmw_subscription_t *, rmw_event_type_t, uint32_t,
    rmw_status_event_callback_t, void *) = nullptr;
constexpr rmw_ret_t (*kRegisterPublisherEvent)(
    const rmw_publisher_t *, rmw_event_type_t, uint32_t,
    rmw_status_event_callback_t, void *) = nullptr;
constexpr rmw_ret_t (*kAssertPublisherLiveliness)(
    const rmw_publisher_t *) = nullptr;

const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    /*create_session*/            session_create,
    /*destroy_session*/           session_destroy,
    /*drive_io*/                  session_drive_io,

    /* ---- Publisher ---- */
    /*create_publisher*/          publisher_create,
    /*destroy_publisher*/         publisher_destroy,
    /*publish*/               publisher_publish_raw,

    /* ---- Subscription ---- */
    /*create_subscription*/       subscription_create,
    /*destroy_subscription*/      subscription_destroy,
    /*take*/                      subscription_take,
    /*has_data*/                  subscription_has_data,

    /* ---- Service ---- */
    /*create_service*/            service_create,
    /*destroy_service*/           service_destroy,
    /*take_request*/             service_take_request,
    /*has_request*/               service_has_request,
    /*send_response*/                service_send_response,

    /* ---- Client ---- */
    /*create_client*/             client_create,
    /*destroy_client*/            client_destroy,
    /* Phase 130.8 — non-blocking send/recv split; phase-301 deleted
     * the deprecated blocking call_raw slot, so this pair is the one
     * request/reply path. */
    /*send_request*/          service_send_request_raw,
    /*take_response*/            service_take_response,

    /* ---- Phase 108 event hooks (deferred) ---- */
    /*subscription_event_init*/ kRegisterSubscriptionEvent,
    /* Issue 0780 — the poll half of the status-event surface, IMPLEMENTED.
     * `dds_get_*_status` resets its change counters as it reads them, which is
     * take semantics already, so this needs no listener, no buffer and no
     * lock — and it avoids the thing that made the decline wrong: a listener
     * fires on Cyclone's worker thread and this backend's `drive_io` is a
     * sleep with nowhere to defer to. `*_event_init` stays NULL. */
    /*subscription_take_event*/ subscription_take_event,
    /*publisher_take_event*/  publisher_take_event,
    /*publisher_event_init*/  kRegisterPublisherEvent,
    /*publisher_assert_liveliness*/ kAssertPublisherLiveliness,
    /* ---- Phase 110.0 + 104.C.6.b hooks (deferred) ---- */
    /*next_deadline_ms*/          nullptr,
    /* Phase 124.B.1 — Cyclone DDS has its own background threads
     * for sample arrival + matched-entity events; wiring the wake
     * callback into those listeners is a follow-up (lives in the
     * listener-installation path, not this static vtable). nullptr
     * today; runtime drains on deadline-bound cv-wait boundary. */
    /*set_wake_callback*/         session_set_wake_callback,

    /* Phase 124.A — zero-copy ABI. Cyclone DDS supports loan via
     * dds_loan_sample / dds_return_loan; wire-up is a follow-up
     * (track under 124.A.5). nullptr today → runtime falls back to
     * the arena staging-buffer path on this backend. */
    /*borrow_loaned_message*/                  nullptr,
    /*publish_loaned_message*/                nullptr,
    /*return_loaned_message_from_publisher*/               nullptr,
    /*take_loaned_message*/       nullptr,
    /*return_loaned_message_from_subscription*/               nullptr,

    /* Phase 124.C — service availability probe. Deferred until the
     * Cyclone DDS built-in topic readers are wired through (matches
     * the 124.C.2 DDS blocker). nullptr → runtime surfaces
     * NROS_RMW_RET_UNSUPPORTED, no stub. */
    /*service_server_is_available*/  nullptr,

    /* Phase 124.D.3 — native batch take. Cyclone provides
     * `dds_take(reader, buf, info, count, maxs)` as a single-call
     * batch API; we wrap it in subscription_take_sequence with
     * CDR re-serialisation per slot. */
    /*take_sequence*/             subscription_take_sequence,

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

    /* issue 0800 — the table now runs to the END of the struct rather than
     * stopping early and leaning on value-initialisation. Positional init
     * cannot skip: reaching `set_log_severity` (slot 73) means naming every
     * slot before it. `check-rmw-vtable-order` verifies these comments against
     * the header's field order, so a slot inserted upstream cannot silently
     * shift the ones below it. */
    /*get_implementation_identifier*/ nullptr,
    /*get_serialization_format*/ nullptr,
    /*feature_supported*/ nullptr,
    /*get_gid_for_publisher*/ cyclone_get_gid_for_publisher,
    /*publisher_count_matched_subscriptions*/ cyclone_publisher_count_matched_subscriptions,
    /*subscription_count_matched_publishers*/ cyclone_subscription_count_matched_publishers,
    /*publisher_get_actual_qos*/ cyclone_publisher_get_actual_qos,
    /*subscription_get_actual_qos*/ cyclone_subscription_get_actual_qos,
    /*client_request_publisher_get_actual_qos*/ cyclone_client_request_publisher_get_actual_qos,
    /*client_response_subscription_get_actual_qos*/ cyclone_client_response_subscription_get_actual_qos,
    /*service_request_subscription_get_actual_qos*/ cyclone_service_request_subscription_get_actual_qos,
    /*service_response_publisher_get_actual_qos*/ cyclone_service_response_publisher_get_actual_qos,
    /*publisher_wait_for_all_acked*/ nullptr,
    /*take_with_info*/ nullptr,
    /*take_loaned_message_with_info*/ nullptr,
    /*service_set_on_new_request_callback*/ nullptr,
    /*client_set_on_new_response_callback*/ nullptr,
    /*subscription_set_on_new_message_callback*/ nullptr,
    /*get_node_names*/ nullptr,
    /*get_topic_names_and_types*/ nullptr,
    /*get_service_names_and_types*/ nullptr,
    /*get_publisher_names_and_types_by_node*/ nullptr,
    /*get_subscriber_names_and_types_by_node*/ nullptr,
    /*get_service_names_and_types_by_node*/ nullptr,
    /*get_client_names_and_types_by_node*/ nullptr,
    /*get_publishers_info_by_topic*/ nullptr,
    /*get_subscriptions_info_by_topic*/ nullptr,
    /*subscription_set_content_filter*/ nullptr,
    /*subscription_get_content_filter*/ nullptr,
    /*publisher_get_network_flow_endpoints*/ nullptr,
    /*subscription_get_network_flow_endpoints*/ nullptr,
    /*count_publishers*/ nullptr,
    /*count_subscribers*/ nullptr,
    /*node_get_graph_guard_condition*/ nullptr,
    /*create_node*/ nullptr,
    /*destroy_node*/ nullptr,
    /*set_log_severity*/ cyclone_set_log_severity,
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

extern "C" rmw_ret_t nros_rmw_cyclonedds_register(void) {
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
