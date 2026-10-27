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

/* Issue 2107 — the six `*_get_actual_qos` slots: one helper per entity kind
 * (session.c), six thunks. See `xrce_actual_qos_topic` /
 * `xrce_actual_qos_service` for what is known and why — and for what was
 * measured: a pub/sub entity holds what the create sent, a requester/replier
 * holds the Agent's fixed profile whatever was asked. */
#define XRCE_ACTUAL_QOS_BODY(state_type, entity, expr)                                            \
    do {                                                                                           \
        if ((entity) == NULL || (entity)->backend_data == NULL || qos == NULL) {                   \
            return NROS_RMW_RET_INVALID_ARGUMENT;                                                  \
        }                                                                                          \
        const state_type* s = (const state_type*)(entity)->backend_data;                           \
        xrce_session_lock(s->session_state);                                                       \
        expr;                                                                                      \
        xrce_session_unlock(s->session_state);                                                     \
        return NROS_RMW_RET_OK;                                                                    \
    } while (0)

static rmw_ret_t xrce_publisher_get_actual_qos(const rmw_publisher_t* publisher,
                                               rmw_qos_profile_t* qos) {
    XRCE_ACTUAL_QOS_BODY(xrce_publisher_state, publisher,
                         xrce_actual_qos_topic(&s->sent_qos, s->avoid_ros, qos));
}

static rmw_ret_t xrce_subscription_get_actual_qos(const rmw_subscription_t* subscription,
                                                  rmw_qos_profile_t* qos) {
    XRCE_ACTUAL_QOS_BODY(xrce_subscriber_state, subscription,
                         xrce_actual_qos_topic(&s->sent_qos, s->avoid_ros, qos));
}

static rmw_ret_t xrce_client_request_publisher_get_actual_qos(const rmw_client_t* client,
                                                              rmw_qos_profile_t* qos) {
    XRCE_ACTUAL_QOS_BODY(xrce_service_client_state, client,
                         xrce_actual_qos_service(true, s->avoid_ros, qos));
}

static rmw_ret_t xrce_client_response_subscription_get_actual_qos(const rmw_client_t* client,
                                                                  rmw_qos_profile_t* qos) {
    XRCE_ACTUAL_QOS_BODY(xrce_service_client_state, client,
                         xrce_actual_qos_service(false, s->avoid_ros, qos));
}

static rmw_ret_t xrce_service_request_subscription_get_actual_qos(const rmw_service_t* service,
                                                                  rmw_qos_profile_t* qos) {
    XRCE_ACTUAL_QOS_BODY(xrce_service_server_state, service,
                         xrce_actual_qos_service(false, s->avoid_ros, qos));
}

static rmw_ret_t xrce_service_response_publisher_get_actual_qos(const rmw_service_t* service,
                                                                rmw_qos_profile_t* qos) {
    XRCE_ACTUAL_QOS_BODY(xrce_service_server_state, service,
                         xrce_actual_qos_service(true, s->avoid_ros, qos));
}

/* RFC-0088 D4 / phase-421 W2 — the wire encoding, as the cross-image identity
 * STRING (the `u8` discriminant beside it is image-local and never crosses).
 *
 * micro-XRCE-DDS-Client serialises with micro-CDR, whose output is OMG CDR:
 * the Agent forwards those bytes to a DDS domain unchanged, so an XRCE image
 * and a host ROS 2 node speak the same encoding. `"cdr"` is therefore a fact
 * about the wire, not about which library wrote it. */
static const char* xrce_get_serialization_format(void) {
    return "cdr";
}

/* The backend's name: the registry name, one spelling for both, so the name
 * `rmw_implementation_identifier()` reports and the name `rmw("xrce")` binds
 * cannot disagree. Not `rmw_microxrcedds` — this is a different client of the
 * same Agent protocol (`rmw_vtable.h`, `get_implementation_identifier`). */
static const char* const kXrceImplementationIdentifier = "xrce";
static const char* xrce_get_implementation_identifier(void) {
    return kXrceImplementationIdentifier;
}

/* Issue 2110 — `get_gid_for_publisher` / `get_gid_for_client`.
 *
 * The XRCE protocol hands the client no GUID for an entity the Agent creates,
 * but graph.c derives each tracked endpoint's DDS GID (issue 1292: the probed
 * participant prefix plus the Agent's per-participant endpoint counter) and
 * publishes it on `ros_discovery_info`. That is the identity a stock peer
 * prints in `ros2 topic info --verbose`, so it is the one answered here — not
 * stock `rmw_microxrcedds`'s copy of the client-local XRCE object id, which
 * matches no DDS GID. A client's identity is its REQUEST writer: the GUID a
 * server reads in the request id (`take_request_with_info`).
 *
 * UNSUPPORTED exactly when the endpoint is not tracked, never a guessed
 * prefix. The identifier is the one `get_implementation_identifier` returns,
 * so two gids naming one endpoint compare equal (`rmw_compare_gids_equal`
 * checks the string first). */
static rmw_ret_t xrce_fill_gid(xrce_session_state_t* st, const xrce_graph_endpoint* ep,
                               rmw_gid_t* gid) {
    xrce_session_lock(st);
    bool known = xrce_graph_endpoint_gid(st, ep, gid->data);
    xrce_session_unlock(st);
    if (!known) {
        return NROS_RMW_RET_UNSUPPORTED;
    }
    gid->implementation_identifier = kXrceImplementationIdentifier;
    return NROS_RMW_RET_OK;
}

static rmw_ret_t xrce_get_gid_for_publisher(const rmw_publisher_t* publisher, rmw_gid_t* gid) {
    if (publisher == NULL || publisher->backend_data == NULL || gid == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const xrce_publisher_state* ps = (const xrce_publisher_state*)publisher->backend_data;
    return xrce_fill_gid(ps->session_state, &ps->graph_writer, gid);
}

static rmw_ret_t xrce_get_gid_for_client(const rmw_client_t* client, rmw_gid_t* gid) {
    if (client == NULL || client->backend_data == NULL || gid == NULL) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const xrce_service_client_state* cs = (const xrce_service_client_state*)client->backend_data;
    return xrce_fill_gid(cs->session_state, &cs->graph_writer, gid);
}

/* Phase 108 event hooks left NULL until a follow-up phase wires
 * micro-XRCE-DDS-Client status callbacks through to the runtime's
 * status-event surface. */
static const nros_rmw_vtable_t kVtable = {
    /* ---- Session lifecycle ---- */
    .create_session = xrce_session_create,
    .destroy_session = xrce_session_destroy,
    .drive_io = xrce_session_drive_io,

    /* ---- Node (issue 1292) ----
     * A node is what `ros_discovery_info` lists, so it is declared here and
     * published by graph.c, exactly as Cyclone has since issue 1269. */
    .create_node = xrce_node_create,
    .destroy_node = xrce_node_destroy,

    /* ---- Graph (issue 2106) ----
     * Node names are DATA on `ros_discovery_info`, which this backend reads
     * through the Agent like any topic (graph.c). The other graph slots stay
     * NULL: topic names, types, endpoint info and counts live in the Agent
     * participant's discovery state, which the protocol gives no read on. */
    .get_node_names = xrce_get_node_names,
    .node_get_graph_guard_condition = xrce_node_get_graph_guard_condition,

    /* ---- Publisher ---- */
    .create_publisher = xrce_publisher_create,
    .destroy_publisher = xrce_publisher_destroy,
    .publish = xrce_publisher_publish_raw,

    /* ---- Subscription ---- */
    .create_subscription = xrce_subscription_create,
    .destroy_subscription = xrce_subscription_destroy,
    .take = xrce_subscription_take,
    .has_data = xrce_subscription_has_data,

    /* ---- Service ---- */
    .create_service = xrce_service_create,
    .destroy_service = xrce_service_destroy,
    .take_request = xrce_service_take_request,
    /* Issue 2081 — the request's SampleIdentity (client writer GUID +
     * sequence number); no timestamps reach the client library. */
    .take_request_with_info = xrce_service_take_request_with_info,
    .has_request = xrce_service_has_request,
    .send_response = xrce_service_send_response,

    /* ---- Client ---- */
    .create_client = xrce_client_create,
    .destroy_client = xrce_client_destroy,

    /* Phase 130.4 — non-blocking send/recv split. Lets the
     * executor's spin loop poll for a late-arriving reply without
     * re-sending the request (Phase 127.C.4 C++ action send_goal
     * root cause). Phase-301 deleted the deprecated blocking
     * call_raw slot; this pair is the one request/reply path. */
    .send_request = xrce_service_send_request_raw,
    .take_response = xrce_service_take_response,

    /* ---- Phase 108 / 110.0 / 104.C.6.b hooks (deferred) ---- */
    .subscription_event_init = NULL,
    .publisher_event_init = NULL,
    .publisher_assert_liveliness = NULL,
    .next_deadline_ms = NULL,
    /* Phase 124.B.1 — XRCE has no asynchronous notify path
     * (XRCE-DDS-Client is poll-driven via xrce_session_drive_io).
     * NULL = runtime drains this backend on its deadline-bound
     * cv-wait boundary; same-thread setters (Executor::wake, halt)
     * still drive the wake_cv. */
    .set_wake_callback = NULL,

    /* Phase 124.A — zero-copy ABI. XRCE-DDS-Client uses micro-CDR
     * with caller-provided staging buffers; loan/borrow would
     * require a per-publisher arena equivalent. Leave NULL; runtime
     * falls back to the staging-buffer path. */
    .borrow_loaned_message = NULL,
    .publish_loaned_message = NULL,
    .return_loaned_message_from_publisher = NULL,
    .take_loaned_message = NULL,
    .return_loaned_message_from_subscription = NULL,

    /* Phase 124.C / phase-428 W13.c — service availability probe: NULL,
     * DELIBERATELY, and revisited. The Agent owns the DDS participant, so
     * matched-endpoint state lives on the Agent side and micro-XRCE-DDS-Client
     * has no read for it (no `dds_get_matched_*`, no built-in topic readers,
     * no participant enumeration). What the client CAN see — the Agent
     * acknowledged `create_requester` — says only that the Agent built the
     * client's own endpoints, nothing about a server. Answering from that
     * would be issue 1087's "yes without asking" one layer down. NULL is how
     * the runtime says `Err(Unsupported)` ("cannot know"), and every wait loop
     * then waits its budget rather than sending into the void. */
    .service_server_is_available = NULL,

    /* Phase 124.D — native batch take. XRCE delivers one sample per
     * topic callback into a single-slot inbox; no native take_n.
     * Leave NULL → runtime emits the `take` loop fallback. */
    .take_sequence = NULL,

    /* Issue 2109 — per-entity new-data callbacks. `set_wake_callback` above
     * stays NULL (nothing arrives outside `drive_io`), but every arrival
     * callback names its entity, so these fire from inside `drive_io`. */
    .subscription_set_on_new_message_callback = xrce_subscription_set_on_new_message_callback,
    .service_set_on_new_request_callback = xrce_service_set_on_new_request_callback,
    .client_set_on_new_response_callback = xrce_client_set_on_new_response_callback,

    /* Phase 124.E.3 — streamed publish via uxr_prepare_output_stream
     * (writes the payload straight into the reliable output stream,
     * no per-publisher staging buffer). */
    .publish_streamed = xrce_publisher_publish_streamed,

    /* Phase 124.F.2 — connectivity probe via uxr_ping_agent_session. */
    .ping_session = xrce_session_ping,

    /* Issue 2107 — QoS read-back (see `XRCE_ACTUAL_QOS_BODY`). */
    .publisher_get_actual_qos = xrce_publisher_get_actual_qos,
    .subscription_get_actual_qos = xrce_subscription_get_actual_qos,
    .client_request_publisher_get_actual_qos = xrce_client_request_publisher_get_actual_qos,
    .client_response_subscription_get_actual_qos =
        xrce_client_response_subscription_get_actual_qos,
    .service_request_subscription_get_actual_qos =
        xrce_service_request_subscription_get_actual_qos,
    .service_response_publisher_get_actual_qos = xrce_service_response_publisher_get_actual_qos,

    /* Phase 231 (RFC-0038) — zero-copy in-place take over the XRCE static
     * ring (the staged `entry->data`), so the executor's in-place dispatch
     * borrows the bytes instead of copying into an arena buffer. */
    .subscription_supports_in_place = xrce_subscription_supports_in_place,
    .process_raw_in_place = xrce_subscription_process_raw_in_place,

    /* RFC-0088 D4 — the backend's wire encoding, per session. */
    .get_serialization_format = xrce_get_serialization_format,
    .get_implementation_identifier = xrce_get_implementation_identifier,

    /* issue 1329 — this backend answers for ITSELF instead of the cffi
     * route answering a union on every C backend's behalf. */
    .supported_qos_policies = xrce_supported_qos_policies,

    /* Issue 2110 — the DDS GID graph.c derives (see `xrce_fill_gid`). */
    .get_gid_for_publisher = xrce_get_gid_for_publisher,
    .get_gid_for_client = xrce_get_gid_for_client,
};

rmw_ret_t nros_rmw_xrce_register(void) {
    /* Phase 104.B.2 — register under the canonical name "xrce" so
     * bridge code (and `Executor::create_node_with_rmw("name", "xrce",
     * ...)`) can resolve this backend through the named registry. */
    return nros_rmw_cffi_register_named(kXrceImplementationIdentifier, &kVtable);
}

/* Phase 115.K.2.5.2 — auto-register on library load.
 *
 * The C/C++ APIs go through `rclc_support_init` (C) or `nros::init`
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
__attribute__((constructor)) static void nros_rmw_xrce_register_ctor(void) {
    (void)nros_rmw_cffi_register_named(kXrceImplementationIdentifier, &kVtable);
}
#endif
