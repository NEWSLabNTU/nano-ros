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
#include "nros/rmw_vtable.h"

#include <atomic>
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
    // phase-521 W8 (d), issue 2218 — the executor's wake. Read from the
    // broker's callback context, so the pair is atomic: `ctx` is stored
    // before `cb` and read after it.
    std::atomic<void (*)(void *)> wake_cb{nullptr};
    std::atomic<void *> wake_ctx{nullptr};
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

rmw_ret_t session_create(const char * /*locator*/, uint8_t /*mode*/,
                            uint32_t domain_id, const char *node_name,
                            const rmw_session_options_t *options,
                            rmw_session_t *out) {
    if (out == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // phase-206 W3 — a property this backend cannot honour is REFUSED, not
    // dropped. uORB is an in-process broker with no run-time option set at
    // all, so every property name is unknown to it; saying so is the only
    // honest answer, and a silent drop reads as "it was applied".
    if (options != nullptr && options->property_count != 0) {
        return NROS_RMW_RET_UNSUPPORTED;
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

rmw_ret_t session_destroy(rmw_session_t *session) {
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

rmw_ret_t session_set_wake_callback(rmw_session_t *session, void (*cb)(void *ctx), void *ctx) {
    if (session == nullptr || session->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SessionState *>(session->backend_data);
    state->wake_cb.store(nullptr, std::memory_order_release);
    state->wake_ctx.store(ctx, std::memory_order_release);
    state->wake_cb.store(cb, std::memory_order_release);
    return NROS_RMW_RET_OK;
}

void session_wake(void *session_backend_data) {
    if (session_backend_data == nullptr) {
        return;
    }
    auto *state = static_cast<SessionState *>(session_backend_data);
    void (*cb)(void *) = state->wake_cb.load(std::memory_order_acquire);
    if (cb != nullptr) {
        cb(state->wake_ctx.load(std::memory_order_acquire));
    }
}

// phase-521 W8 (d), issue 2218 — RFC-0113 D4. With the PX4 push-wake glue
// linked, every subscription registers a `SubscriptionCallback` and an
// arrival calls this session's wake (and its per-entity callback): Notify.
// Without it (the host test double, `NROS_RMW_UORB_BUILD_PX4_GLUE=OFF`) a
// subscription can only be polled, which D5 refuses at open.
rmw_ret_t session_readiness_source(const rmw_session_t *session, uint8_t *out_kind,
                                   int32_t * /*out_handle*/) {
    if (session == nullptr || out_kind == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
#ifdef NROS_RMW_UORB_PX4_GLUE
    *out_kind = NROS_RMW_READINESS_NOTIFY;
#else
    *out_kind = NROS_RMW_READINESS_NONE;
#endif
    return NROS_RMW_RET_OK;
}

rmw_ret_t session_drive_io(rmw_session_t *session,
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
