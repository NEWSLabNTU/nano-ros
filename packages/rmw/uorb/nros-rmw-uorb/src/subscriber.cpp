// Phase 115.K.4.2-subscriber — subscriber data plane.
//
// Wires `orb_subscribe_multi` / `orb_check` / `orb_copy` /
// `orb_unsubscribe`, plus `nros_orb_register_callback` for
// push-wake when running inside a real PX4 build (K.4.2-sub-push).
//
// Two-tier delivery:
//   - **Fast path (PX4 build):** the broker's workqueue thread
//     fires `subscriber_ready_callback` → flips
//     `SubscriberState::ready` atomically. `has_data` /
//     `try_recv_raw` short-circuit on the flag, skipping the
//     `orb_check` syscall on the common "no data" branch.
//   - **Slow path (host build / push-wake unavailable):**
//     `nros_orb_register_callback` returns -1, the flag stays
//     pinned to true, and `has_data` / `try_recv_raw` fall
//     through to `orb_check` every time. Same behaviour the
//     pre-push-wake K.4.2 build had.
//
// Storage discipline:
//   - `create_subscription` looks up the topic in the K.4.3
//     registry, calls `orb_subscribe_multi(meta, 0)`, allocates a
//     `SubscriberState` holding the subscription handle + ready
//     flag, attempts `nros_orb_register_callback`.
//   - `try_recv_raw` fast-checks the flag, then `orb_check`, then
//     `orb_copy`. BUFFER_TOO_SMALL when `buf_len < meta->o_size`
//     and DOES NOT drain (retry-safe).
//   - `has_data` returns the flag (or runs `orb_check` on the
//     slow path).
//   - `destroy_subscription` unregisters callback + unsubscribes +
//     frees.

#include "internal.hpp"
#include "nros_rmw_uorb_registry.h"
#include "uorb_abi.hpp"

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <atomic>
#include <cstdlib>
#include <new>

namespace nros_rmw_uorb {

namespace {

struct SubscriberState {
    const struct orb_metadata *meta;
    int sub_handle;
    // `true` whenever the broker signals fresh data and after the
    // initial create (so the first poll triggers an orb_check that
    // surfaces any sample latched between subscribe + first call).
    // On the slow path (callback registration failed) we pin this
    // to `true` so try_recv_raw always falls through to orb_check.
    std::atomic<bool> ready;
    // `true` if `nros_orb_register_callback` succeeded — used by
    // destroy to decide whether to unregister.
    bool callback_active;
};

extern "C" void subscriber_ready_callback(void *arg) {
    auto *state = static_cast<SubscriberState *>(arg);
    state->ready.store(true, std::memory_order_release);
}

} // namespace

rmw_ret_t subscription_create(const rmw_node_t* node,
                                 const char *topic_name,
                                 const char * /*type_name*/,
                                 const char * /*type_hash*/,
                                 uint32_t /*domain_id*/,
                                 const rmw_qos_profile_t * /*qos*/,
                                 const rmw_subscription_options_t * /*options*/,
                                 rmw_subscription_t *out) {
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
    state->callback_active = false;
    // K.4.2-sub-push: try push-wake. Failure leaves callback_active
    // = false; we pin `ready` to true so try_recv_raw degrades
    // gracefully to the slow polling path.
    int reg_rc = nros_orb_register_callback(meta,
                                            /*instance=*/0,
                                            handle,
                                            subscriber_ready_callback,
                                            state);
    if (reg_rc == 0) {
        state->callback_active = true;
        // Start "ready" so the first poll surfaces any sample the
        // broker latched between subscribe + callback install.
        state->ready.store(true, std::memory_order_relaxed);
    } else {
        // Slow path: pin ready so has_data / try_recv_raw never
        // short-circuit on the flag.
        state->ready.store(true, std::memory_order_relaxed);
    }
    out->backend_data = state;
    out->can_loan_messages = false;
    return NROS_RMW_RET_OK;
}

rmw_ret_t subscription_destroy(rmw_subscription_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    int rc = 0;
    if (state->callback_active && nros_orb_unregister_callback(state->sub_handle) != 0) {
        rc = -1;
    }
    if (orb_unsubscribe(state->sub_handle) != 0) {
        rc = -1;
    }
    state->~SubscriberState();
    std::free(state);
    subscriber->backend_data = nullptr;
    return rc == 0 ? NROS_RMW_RET_OK : NROS_RMW_RET_ERROR;
}

rmw_ret_t subscription_take(const rmw_subscription_t *subscriber,
                                 uint8_t *buf, size_t buf_len,
                                 size_t *out_len, bool *taken) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (buf == nullptr && buf_len != 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    // Phase 376 W3.b/W3.d step A — upstream `rmw_take`'s shape.
    if (out_len == nullptr || taken == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    // Fast path: on the push-wake build, `ready` flips only when
    // the broker fires our callback. Skip the orb_check syscall
    // when we know nothing is pending. On the slow path the flag
    // is pinned to `true` so we always fall through.
    if (state->callback_active
        && !state->ready.load(std::memory_order_acquire)) {
        *taken = false;
        return NROS_RMW_RET_OK;
    }
    bool updated = false;
    if (orb_check(state->sub_handle, &updated) != 0) {
        return NROS_RMW_RET_ERROR;
    }
    if (!updated) {
        // Re-arm: nothing in the queue; reset the flag so the next
        // callback fires us back into the fast path.
        if (state->callback_active) {
            state->ready.store(false, std::memory_order_release);
        }
        *taken = false;
        return NROS_RMW_RET_OK;
    }
    if (buf_len < state->meta->o_size) {
        // Don't drain — caller may retry with a larger buffer.
        return NROS_RMW_RET_BUFFER_TOO_SMALL;
    }
    if (orb_copy(state->meta, state->sub_handle, buf) != 0) {
        return NROS_RMW_RET_ERROR;
    }
    if (state->callback_active) {
        // Sample drained; clear the flag. Next sample re-arms via
        // the broker callback.
        state->ready.store(false, std::memory_order_release);
    }
    *out_len = static_cast<size_t>(state->meta->o_size);
    *taken = true;
    return NROS_RMW_RET_OK;
}

rmw_ret_t subscription_has_data(rmw_subscription_t *subscriber, bool *out_has_data) {
    // Phase 376 W3.d step A — flag out, status returned. A failing `orb_check`
    // used to be reported as "no data"; it is now an error.
    if (out_has_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    // Fast path: the callback flag is the authoritative signal.
    if (state->callback_active
        && !state->ready.load(std::memory_order_acquire)) {
        *out_has_data = false;
        return NROS_RMW_RET_OK;
    }
    bool updated = false;
    if (orb_check(state->sub_handle, &updated) != 0) {
        return NROS_RMW_RET_ERROR;
    }
    *out_has_data = updated;
    return NROS_RMW_RET_OK;
}

} // namespace nros_rmw_uorb
