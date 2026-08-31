// Phase 115.K.4.2 — publisher data plane.
//
// Wires `orb_advertise_multi` / `orb_publish` / `orb_unadvertise`.
// Topic-name → `orb_metadata *` resolution goes through the
// K.4.3 registry (`nros_rmw_uorb_lookup_topic`).
//
// Storage discipline:
//   - `create_publisher` allocates a `PublisherState` carrying the
//     metadata pointer + advert handle + instance index.
//   - `publish_raw` validates payload length against the topic's
//     declared message size before calling `orb_publish`; uORB
//     silently truncates / overruns otherwise.
//   - `destroy_publisher` unadvertises + frees.

#include "internal.hpp"
#include "nros_rmw_uorb_registry.h"
#include "uorb_abi.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <cstdlib>
#include <cstring>
#include <new>

namespace nros_rmw_uorb {

namespace {

struct PublisherState {
    const struct orb_metadata *meta;
    orb_advert_t advert;
    int instance;
};

} // namespace

rmw_ret_t publisher_create(const rmw_node_t* node,
                                const rmw_message_type_support_t * /*type_support*/,
                                const char *topic_name,
                                uint32_t /*domain_id*/,
                                const rmw_qos_profile_t * /*qos*/,
                                const rmw_publisher_options_t * /*options*/,
                                rmw_publisher_t *out) {
    // Phase 376 W5/B1 — the entity is created ON ITS NODE, as upstream does.
    // The node carries the route to its session (our `context`).
    if (node == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    rmw_session_t* session = node->session;
    if (session == nullptr || session->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (out == nullptr || topic_name == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    const struct orb_metadata *meta = nros_rmw_uorb_lookup_topic(topic_name);
    if (meta == nullptr) {
        // The host module forgot to register this topic. Distinct
        // from UNSUPPORTED so the caller can distinguish "no such
        // topic" from "backend doesn't do publishers."
        return NROS_RMW_RET_TOPIC_NAME_INVALID;
    }
    auto *state = static_cast<PublisherState *>(std::malloc(sizeof(PublisherState)));
    if (state == nullptr) {
        return NROS_RMW_RET_BAD_ALLOC;
    }
    new (state) PublisherState();
    state->meta = meta;
    state->advert = nullptr;
    state->instance = 0;
    // Lazy advertise: orb_advertise_multi needs a sample payload to
    // initialise the topic queue. We don't have one until the user
    // calls publish_raw; defer the actual advertise to first
    // publish. This mirrors the legacy `nros-rmw-uorb` Rust impl.
    out->backend_data = state;
    out->can_loan_messages = false;
    return NROS_RMW_RET_OK;
}

rmw_ret_t publisher_destroy(rmw_publisher_t *publisher) {
    if (publisher == nullptr || publisher->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<PublisherState *>(publisher->backend_data);
    // Phase 376 W5 — uORB reports; we used to discard it. Teardown still
    // completes: the advertisement is what leaks, and freeing the state
    // regardless keeps one leak from becoming two.
    int unadvertise_rc = 0;
    if (state->advert != nullptr) {
        unadvertise_rc = orb_unadvertise(state->advert);
    }
    state->~PublisherState();
    std::free(state);
    publisher->backend_data = nullptr;
    return unadvertise_rc == 0 ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

rmw_ret_t publisher_publish_raw(const rmw_publisher_t *publisher,
                                     rmw_byte_span_t payload) {
    /* phase-406 W2 — one span in, the old two names out. */
    const uint8_t *data = payload.data;
    size_t len = payload.len;
    if (publisher == nullptr || publisher->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (data == nullptr && len != 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<PublisherState *>(publisher->backend_data);
    if (len < state->meta->o_size) {
        // uORB advertisers expect exactly `o_size` bytes — a short
        // payload would read past the caller buffer inside
        // orb_publish.
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    if (state->advert == nullptr) {
        // First publish: advertise now using the supplied sample
        // as the initial value (uORB stashes it in the queue).
        state->advert = orb_advertise_multi(state->meta, data, &state->instance);
        if (state->advert == nullptr) {
            return NROS_RMW_RET_ERROR;
        }
        return NROS_RMW_RET_OK;
    }
    if (orb_publish(state->meta, state->advert, data) != 0) {
        return NROS_RMW_RET_ERROR;
    }
    return NROS_RMW_RET_OK;
}

} // namespace nros_rmw_uorb
