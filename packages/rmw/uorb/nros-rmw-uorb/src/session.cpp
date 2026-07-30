// Session lifecycle.
//
// uORB has no global session object — the broker is process-wide and
// shared across all advertisers/subscribers. The session struct here
// just carries node-name / namespace strings used by the diagnostic
// surface (PX4_INFO logging, topic-key salt for service-over-topics
// if K.4.4 chooses that path).
//
// `drive_io` is a no-op: uORB delivery is push-based via
// `orb_register_callback` (fires on the publisher's workqueue
// context), so the runtime's drive_io has no I/O to pump for this
// backend.
//
// Phase 115.K.4.1 — session lifecycle wired; entity-create paths
// (K.4.2/K.4.3) still return UNSUPPORTED in publisher.cpp /
// subscriber.cpp / service.cpp.

#include "internal.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <new>

namespace nros_rmw_uorb {

namespace {

// Fixed-capacity inline strings keep the session state allocator-
// free in the small-name case (matches PX4 module conventions —
// node names are short, like `commander`, `vehicle_status`). If a
// caller exceeds the cap, truncate; we don't fail the open since the
// strings are diagnostic-only.
constexpr size_t kMaxNodeName  = 64;
constexpr size_t kMaxNamespace = 64;

struct SessionState {
    char node_name[kMaxNodeName];
    char namespace_[kMaxNamespace];
    uint32_t domain_id;
};

// Copy a null-terminated C string into a fixed buffer, truncating to
// `buf_len - 1` chars and always null-terminating. NULL src writes
// an empty string.
void copy_truncated(char *buf, size_t buf_len, const char *src) {
    if (buf_len == 0) {
        return;
    }
    if (src == nullptr) {
        buf[0] = '\0';
        return;
    }
    size_t n = 0;
    while (n + 1 < buf_len && src[n] != '\0') {
        buf[n] = src[n];
        ++n;
    }
    buf[n] = '\0';
}

} // namespace

nros_rmw_ret_t session_create(const char * /*locator*/, uint8_t /*mode*/,
                            uint32_t domain_id, const char *node_name,
                            nros_rmw_session_t *out) {
    if (out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // uORB ignores the locator (in-process broker) and the session
    // mode (no client/peer distinction). domain_id is stashed for
    // diagnostic readback; uORB itself has no domain segregation.
    auto *state = static_cast<SessionState *>(std::malloc(sizeof(SessionState)));
    if (state == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    new (state) SessionState();
    copy_truncated(state->node_name,  kMaxNodeName,  node_name);
    copy_truncated(state->namespace_, kMaxNamespace, /*namespace not passed in*/ nullptr);
    state->domain_id = domain_id;
    out->backend_data = state;
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_destroy(nros_rmw_session_t *session) {
    if (session == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SessionState *>(session->backend_data);
    if (state != nullptr) {
        state->~SessionState();
        std::free(state);
        session->backend_data = nullptr;
    }
    return NROS_RMW_RET_OK;
}

nros_rmw_ret_t session_drive_io(nros_rmw_session_t *session,
                                int32_t /*timeout_ms*/) {
    if (session == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // No-op for uORB. orb_register_callback fires from the broker's
    // workqueue context and signals the per-subscriber ringbuffer
    // (K.4.2). Nothing for the runtime's drive_io to drain.
    return NROS_RMW_RET_OK;
}

} // namespace nros_rmw_uorb
