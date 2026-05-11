// Phase 115.K.4.2-subscriber — subscriber data plane.
//
// Wires `orb_subscribe_multi` / `orb_check` / `orb_copy` /
// `orb_unsubscribe`. Polling shape rather than callback-driven
// because the cffi `try_recv_raw` contract is non-blocking pull
// anyway; `orb_register_callback` integration (push-wake of the
// runtime's spin loop) is queued as K.4.2-subscriber-push.
//
// Storage discipline:
//   - `create_subscriber` looks up the topic in the K.4.3 registry,
//     calls `orb_subscribe_multi(meta, 0)`, allocates a
//     `SubscriberState` holding the subscription handle.
//   - `try_recv_raw` calls `orb_check`; if updated, `orb_copy`s
//     the sample into the caller buffer. Caller buf must be ≥
//     meta->o_size; short buffers reject with `BUFFER_TOO_SMALL`
//     and DO NOT drain the queue (re-poll succeeds with a larger
//     buf).
//   - `has_data` does `orb_check` and returns the flag.
//   - `destroy_subscriber` calls `orb_unsubscribe` + frees.

#include "internal.hpp"
#include "nros_rmw_uorb_registry.h"
#include "uorb_abi.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <cstdlib>
#include <new>

namespace nros_rmw_uorb {

namespace {

struct SubscriberState {
    const struct orb_metadata *meta;
    int sub_handle;
};

} // namespace

nros_rmw_ret_t subscriber_create(nros_rmw_session_t *session,
                                 const char *topic_name,
                                 const char * /*type_name*/,
                                 const char * /*type_hash*/,
                                 uint32_t /*domain_id*/,
                                 const nros_rmw_qos_t * /*qos*/,
                                 nros_rmw_subscriber_t *out) {
    if (session == nullptr || session->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (out == nullptr || topic_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const struct orb_metadata *meta = nros_rmw_uorb_lookup_topic(topic_name);
    if (meta == nullptr) {
        return NROS_RMW_RET_TOPIC_NAME_INVALID;
    }
    int handle = orb_subscribe_multi(meta, /*instance=*/0);
    if (handle < 0) {
        return NROS_RMW_RET_ERROR;
    }
    auto *state = static_cast<SubscriberState *>(std::malloc(sizeof(SubscriberState)));
    if (state == nullptr) {
        (void)orb_unsubscribe(handle);
        return NROS_RMW_RET_BAD_ALLOC;
    }
    new (state) SubscriberState();
    state->meta = meta;
    state->sub_handle = handle;
    out->backend_data = state;
    out->can_loan_messages = false;
    return NROS_RMW_RET_OK;
}

void subscriber_destroy(nros_rmw_subscriber_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    (void)orb_unsubscribe(state->sub_handle);
    state->~SubscriberState();
    std::free(state);
    subscriber->backend_data = nullptr;
}

int32_t subscriber_try_recv_raw(nros_rmw_subscriber_t *subscriber,
                                uint8_t *buf, size_t buf_len) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (buf == nullptr && buf_len != 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    bool updated = false;
    if (orb_check(state->sub_handle, &updated) != 0) {
        return NROS_RMW_RET_ERROR;
    }
    if (!updated) {
        return NROS_RMW_RET_NO_DATA;
    }
    if (buf_len < state->meta->o_size) {
        // Don't drain — caller may retry with a larger buffer.
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    if (orb_copy(state->meta, state->sub_handle, buf) != 0) {
        return NROS_RMW_RET_ERROR;
    }
    return static_cast<int32_t>(state->meta->o_size);
}

int32_t subscriber_has_data(nros_rmw_subscriber_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return 0;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    bool updated = false;
    if (orb_check(state->sub_handle, &updated) != 0) {
        return 0;
    }
    return updated ? 1 : 0;
}

} // namespace nros_rmw_uorb
