// uORB RMW backend — vtable assembly + register entry point.
//
// Phase 115.K.4.0 (scaffold): every slot points at the matching
// stub in session.cpp / publisher.cpp / subscriber.cpp /
// service.cpp. Stubs return NROS_RMW_RET_UNSUPPORTED so the runtime
// sees a wired-but-inert backend until K.4.1 (session lifecycle),
// K.4.2 (pub/sub data plane), and K.4.3 (type-hash correlation) land.

#include "nros_rmw_uorb.h"

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"

#include "internal.hpp"

namespace {

using namespace nros_rmw_uorb;

/* RFC-0088 D4 / phase-421 W2 — uORB's wire encoding.
 *
 * `"uorb"`, and this is the whole point of the slot. Every other backend in
 * this tree answers `"cdr"`; uORB's payload IS the PX4 message struct, byte for
 * byte, with no encoding step at all (RFC-0011) — publisher and subscriber
 * agree because they were compiled against the same header, not because they
 * agree on a format. Until now that difference lived in prose ("uORB is a
 * special case"); a bridge wiring a uORB session to a CDR one could not see it
 * and forwarded the bytes anyway.
 *
 * The string, not the discriminant, is what crosses an image boundary
 * (RFC-0088 D2): the `u8` is assigned per image and two independently built
 * images would disagree about what a number means. */
static const char *uorb_get_serialization_format(void) { return "uorb"; }

// Positional initialization through `get_serialization_format`, in
// `nros_rmw_vtable_t` field order with NO gaps — every slot after it is left to
// C++ aggregate value-initialization (NULL), which the runtime treats as
// "unsupported." Designated initializers would be cleaner but need C++20;
// this crate is C++14 (CMAKE_CXX_STANDARD 14), so keep the gap-free positional
// form — a skipped slot shifts every later slot and breaks the build.
// Phase-301: the deprecated blocking `call_raw` slot was deleted from the
// vtable, so the positional list used to end at `destroy_client`.
//
// phase-421 W2 (RFC-0088 D4): it now runs 21 slots further, to
// `get_serialization_format`, because positional init cannot SKIP — reaching a
// slot means naming every slot before it. The intervening `nullptr`s carry no
// new decision; each one is the value C++ was already giving them, written
// down. `check-vtable-positional-order` checks these names against the
// header's field order, so a slot inserted upstream cannot silently shift the
// ones below it.
//
// The trailing-NULL `-Wmissing-field-initializers` is the intended shape here.
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wmissing-field-initializers"
const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    /*create_session*/ session_create,
    /*destroy_session*/ session_destroy,
    /*drive_io*/ session_drive_io,
    /* ---- Publisher ---- */
    /*create_publisher*/ publisher_create,
    /*destroy_publisher*/ publisher_destroy,
    /*publish*/ publisher_publish_raw,
    /* ---- Subscription ---- */
    /*create_subscription*/ subscription_create,
    /*destroy_subscription*/ subscription_destroy,
    /*take*/ subscription_take,
    /*has_data*/ subscription_has_data,
    /* ---- Service (uORB: UNSUPPORTED stubs) ---- */
    /*create_service*/ service_create,
    /*destroy_service*/ service_destroy,
    /*take_request*/ service_take_request,
    /*has_request*/ service_has_request,
    /*send_response*/ service_send_response,
    /* ---- Client (uORB: UNSUPPORTED stubs) ---- */
    /*create_client*/ client_create,
    /*destroy_client*/ client_destroy,

    /* ---- Not implemented on uORB; named only so the positional list can
     * reach `get_serialization_format` below. ---- */
    /*send_request*/ nullptr,
    /*take_response*/ nullptr,
    /*subscription_event_init*/ nullptr,
    /*subscription_take_event*/ nullptr,
    /*publisher_take_event*/ nullptr,
    /*publisher_event_init*/ nullptr,
    /*publisher_assert_liveliness*/ nullptr,
    /*next_deadline_ms*/ nullptr,
    /*set_wake_callback*/ nullptr,
    /*borrow_loaned_message*/ nullptr,
    /*publish_loaned_message*/ nullptr,
    /*return_loaned_message_from_publisher*/ nullptr,
    /*take_loaned_message*/ nullptr,
    /*return_loaned_message_from_subscription*/ nullptr,
    /*service_server_is_available*/ nullptr,
    /*take_sequence*/ nullptr,
    /*publish_streamed*/ nullptr,
    /*ping_session*/ nullptr,
    /*subscription_supports_in_place*/ nullptr,
    /*process_raw_in_place*/ nullptr,
    /*get_implementation_identifier*/ nullptr,

    /* RFC-0088 D4 — the one slot uORB answers differently from every other
     * backend, and the reason the slot stopped being decoration. */
    /*get_serialization_format*/ uorb_get_serialization_format,

    /* issue 1329 — the list now runs to the END of the struct. Positional init
     * cannot SKIP, so reaching `supported_qos_policies` means naming every
     * slot before it; the intervening `nullptr`s carry no new decision, each
     * being the value C++ aggregate initialisation was already giving them,
     * written down. `check-vtable-positional-order` holds these comments
     * against the header's field order, so a slot inserted upstream cannot
     * silently shift the ones below it.
     *
     * This is the cost the `required_rx_bytes` header block predicted for
     * "slot 75 on a backend with a C++14 positional initialiser", and it is
     * paid here rather than dodged, because a NULL `supported_qos_policies`
     * DECLARES that the backend honours nothing — which would refuse every
     * PX4 entity that states a profile. */
    /*feature_supported*/ nullptr,
    /*get_gid_for_publisher*/ nullptr,
    /*publisher_count_matched_subscriptions*/ nullptr,
    /*subscription_count_matched_publishers*/ nullptr,
    /* issue 1329 — the depth this backend grants is the TOPIC's `o_queue`,
     * not the caller's request; these two are how the runtime learns that and
     * reports it instead of the grant being a silent clamp. */
    /*publisher_get_actual_qos*/ publisher_get_actual_qos,
    /*subscription_get_actual_qos*/ subscription_get_actual_qos,
    /*client_request_publisher_get_actual_qos*/ nullptr,
    /*client_response_subscription_get_actual_qos*/ nullptr,
    /*service_request_subscription_get_actual_qos*/ nullptr,
    /*service_response_publisher_get_actual_qos*/ nullptr,
    /*publisher_wait_for_all_acked*/ nullptr,
    /*take_with_info*/ nullptr,
    /*take_loaned_message_with_info*/ nullptr,
    /*get_node_names*/ nullptr,
    /*get_topic_names_and_types*/ nullptr,
    /*get_service_names_and_types*/ nullptr,
    /*get_publisher_names_and_types_by_node*/ nullptr,
    /*get_subscriber_names_and_types_by_node*/ nullptr,
    /*get_service_names_and_types_by_node*/ nullptr,
    /*get_client_names_and_types_by_node*/ nullptr,
    /*get_publishers_info_by_topic*/ nullptr,
    /*get_subscriptions_info_by_topic*/ nullptr,
    /*count_publishers*/ nullptr,
    /*count_subscribers*/ nullptr,
    /*node_get_graph_guard_condition*/ nullptr,
    /*create_node*/ nullptr,
    /*destroy_node*/ nullptr,
    /*set_log_severity*/ nullptr,
    /*required_rx_bytes*/ nullptr,
    /*supported_qos_policies*/ supported_qos_policies,
};
#pragma GCC diagnostic pop

} // namespace

extern "C" rmw_ret_t nros_rmw_uorb_register(void) {
    // Issue 0436 — register under the CANONICAL name, not the deprecated unnamed
    // shim (which registers the literal name "default"). Every other backend uses
    // `nros_rmw_cffi_register_named` with its own name, and the shim's own
    // deprecation note says to do this.
    //
    // The name is not cosmetic: it is the ONLY handle for selecting this backend
    // in a multi-backend image — `NodeBuilder().rmw("uorb")` (the PX4 bridge's
    // inward session) and `$NROS_RMW=uorb` both look the name up in the registry.
    // Registered as "default", uORB could never be named by either, which is why
    // the bridge's `rmw("uorb")` bind failed.
    //
    // Single-backend images are unaffected: with one entry the resolver returns it
    // regardless of name (`resolve_backend` → `n == 1` → `Single`).
    return nros_rmw_cffi_register_named("uorb", &kVtable);
}
