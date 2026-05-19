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

#ifdef __ZEPHYR__
#include <zephyr/logging/log.h>
LOG_MODULE_DECLARE(cyclonedds, LOG_LEVEL_INF);
#define NROS_CYC_TRACE(...) LOG_INF(__VA_ARGS__)
#else
#define NROS_CYC_TRACE(...) ((void)0)
#endif

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

    NROS_CYC_TRACE("session_open: domain=%u entering", domain_id);
    auto *state = new (std::nothrow) SessionState();
    if (state == nullptr) {
        NROS_CYC_TRACE("session_open: BAD_ALLOC for SessionState");
        return NROS_RMW_RET_BAD_ALLOC;
    }

    NROS_CYC_TRACE("session_open: calling dds_create_participant");
    dds_entity_t pp = dds_create_participant(domain_id, nullptr, nullptr);
    NROS_CYC_TRACE("session_open: dds_create_participant returned %d", (int)pp);
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
