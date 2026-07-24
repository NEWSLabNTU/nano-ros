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

nros_rmw_ret_t subscription_create(nros_rmw_session_t *session,
                                 const char *topic_name,
                                 const char * /*type_name*/,
                                 const char * /*type_hash*/,
                                 uint32_t /*domain_id*/,
                                 const nros_rmw_qos_t * /*qos*/,
                                 const nros_rmw_subscription_options_t * /*options*/,
                                 nros_rmw_subscription_t *out) {
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

void subscription_destroy(nros_rmw_subscription_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    if (state->callback_active) {
        (void)nros_orb_unregister_callback(state->sub_handle);
    }
    (void)orb_unsubscribe(state->sub_handle);
    state->~SubscriberState();
    std::free(state);
    subscriber->backend_data = nullptr;
}

int32_t subscription_try_recv_raw(nros_rmw_subscription_t *subscriber,
                                uint8_t *buf, size_t buf_len) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    if (buf == nullptr && buf_len != 0) {
        return NROS_RMW_RET_INVALID_ARGUMENT;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    // Fast path: on the push-wake build, `ready` flips only when
    // the broker fires our callback. Skip the orb_check syscall
    // when we know nothing is pending. On the slow path the flag
    // is pinned to `true` so we always fall through.
    if (state->callback_active
        && !state->ready.load(std::memory_order_acquire)) {
        return NROS_RMW_RET_NO_DATA;
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
        return NROS_RMW_RET_NO_DATA;
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
    return static_cast<int32_t>(state->meta->o_size);
}

int32_t subscription_has_data(nros_rmw_subscription_t *subscriber) {
    if (subscriber == nullptr || subscriber->backend_data == nullptr) {
        return 0;
    }
    auto *state = static_cast<SubscriberState *>(subscriber->backend_data);
    // Fast path: the callback flag is the authoritative signal.
    if (state->callback_active
        && !state->ready.load(std::memory_order_acquire)) {
        return 0;
    }
    bool updated = false;
    if (orb_check(state->sub_handle, &updated) != 0) {
        return 0;
    }
    return updated ? 1 : 0;
}

} // namespace nros_rmw_uorb
