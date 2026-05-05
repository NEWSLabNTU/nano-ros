// Cyclone DDS session lifecycle.
//
// `session_open` creates a Cyclone participant on the requested
// domain id. The participant entity is stashed in
// `nros_rmw_session_t::backend_data` via a small heap-allocated state
// struct so future per-session resources (publishers, listeners)
// share the same `void*` slot.
//
// Phase 117.4 — domain config is left at Cyclone's default (the
// `CYCLONEDDS_URI` env var, if set; otherwise built-in defaults). A
// raw `ddsi_config` path mirroring autoware-safety-island's static
// peer list lands in 117.6 once pub/sub needs network tuning.

#include "internal.hpp"

#include <dds/dds.h>

#include <cstdlib>
#include <cstring>
#include <new>

namespace nros_rmw_cyclonedds {

namespace {

struct SessionState {
    dds_entity_t participant{0};
};

inline SessionState *as_state(nros_rmw_session_t *s) {
    return static_cast<SessionState *>(s->backend_data);
}

} // namespace

nros_rmw_ret_t session_open(const char * /*locator*/, uint8_t /*mode*/,
                            uint32_t domain_id, const char * /*node_name*/,
                            nros_rmw_session_t *out) {
    if (out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }

    auto *state = new (std::nothrow) SessionState();
    if (state == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }

    // dds_create_participant takes a `dds_domainid_t` (uint32_t).
    // Cyclone's `DDS_DOMAIN_DEFAULT` (0xFFFFFFFFu) means "pick the
    // domain from the configuration"; explicit zero is domain 0 — the
    // ROS 2 default. We pass through `domain_id` unchanged so the
    // runtime-supplied value (typically `ROS_DOMAIN_ID`) wins.
    dds_entity_t pp = dds_create_participant(domain_id, nullptr, nullptr);
    if (pp < 0) {
        delete state;
        return NROS_RMW_RET_ERROR;
    }
    state->participant = pp;
    out->backend_data  = state;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_close(nros_rmw_session_t *session) {
    if (session == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    SessionState *state = as_state(session);
    if (state == nullptr) {
        return NROS_RMW_RET_OK;  // already closed / never opened
    }
    if (state->participant > 0) {
        // dds_delete on the participant cascades to every child
        // entity (writers, readers, topics) it owns.
        (void) dds_delete(state->participant);
    }
    delete state;
    session->backend_data = nullptr;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_drive_io(nros_rmw_session_t * /*session*/,
                                int32_t /*timeout_ms*/) {
    // Cyclone owns its own RX threads internally — `drive_io` has
    // nothing to pump. Listener trampolines (Phase 117.6) wake the
    // runtime's `Activator` directly from inside Cyclone's worker.
    return NROS_RMW_RET_OK;
}

dds_entity_t session_participant(const nros_rmw_session_t *session) {
    if (session == nullptr || session->backend_data == nullptr) {
        return 0;
    }
    return static_cast<const SessionState *>(session->backend_data)->participant;
}

} // namespace nros_rmw_cyclonedds
