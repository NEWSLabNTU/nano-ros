// Phase 115.K.4.2-subscriber-push — PX4-side push-wake glue.
//
// Compiled only when NROS_RMW_UORB_BUILD_PX4_GLUE=ON (which itself
// implies NROS_RMW_UORB_LINK_PX4=ON), and always by
// `nros_px4_add_module`. Provides strong definitions of
// `nros_orb_register_callback` / `nros_orb_unregister_callback`.
//
// Each registration is one `uORB::SubscriptionCallback` subclass whose
// `call()` — run by the broker in the publisher's context — invokes the C
// callback directly (phase-521 W8 d; it used to hop through a work item on
// `lp_default`, see `CallbackSlotImpl`).
//
// Capacity is bounded at compile time
// (`NROS_RMW_UORB_PX4_MAX_CALLBACKS`, default 64), in a fixed pool with
// `alignas` storage; a slot is constructed when a subscription registers.

#include "uorb_abi.hpp"

#ifndef NROS_RMW_UORB_USE_PX4_HEADER
#error "px4_callback_glue.cpp expects NROS_RMW_UORB_USE_PX4_HEADER (PX4 SDK link mode)"
#endif

#include <px4_platform_common/log.h>
#include <uORB/SubscriptionCallback.hpp>

#include <cstddef>
#include <cstdint>
#include <new>

namespace {

#ifndef NROS_RMW_UORB_PX4_MAX_CALLBACKS
#define NROS_RMW_UORB_PX4_MAX_CALLBACKS 64
#endif

/* issue 1131 — this knob sizes `Slot g_pool[N]`, and 0 is NOT a legal size
 * here, for a reason that is about the LANGUAGE rather than about the runtime.
 *
 * Zero looked arguable when this was written: at 0 the range-`for`s over
 * `g_pool` do not execute, `find_free` (once ~~`find_free_or_construct`~~)
 * returns NULL, `nros_orb_register_callback` returns -1, and `subscriber.cpp`
 * then polled. Since phase-521 W8 (d) it REFUSES that subscription instead
 * (RFC-0113 D5: the session told the runtime it is Notify), so at 0 no
 * subscription could be created at all — a second reason, now, beside the
 * one below.
 *
 * It breaks the BUILD. `Slot g_pool[0]` is a zero-size array, which ISO C++
 * forbids (a GNU extension, unlike the C case issue 1033 measured for the XRCE
 * pools). This TU is compiled `-Wall -Wextra -Wpedantic` by our own
 * CMakeLists, and PX4 builds every module `-Werror` — and PX4 is the ONLY
 * build that compiles this file at all, since it is appended to the sources
 * only under NROS_RMW_UORB_BUILD_PX4_GLUE, which requires
 * NROS_RMW_UORB_LINK_PX4. Measured with the real flags
 * (`g++ -std=gnu++14 -Wall -Wextra -Wpedantic -Werror -fno-exceptions
 * -fno-rtti`): at 0, `error: ISO C++ forbids zero-size array 'g_pool'
 * [-Werror=pedantic]`; at 64, clean.
 *
 * And the saving zero was supposed to buy already has a better spelling that
 * costs nothing: an image that wants polling only sets
 * NROS_RMW_UORB_BUILD_PX4_GLUE=OFF, which drops this whole TU and links
 * `callback_default.cpp`'s stubs — those return -1 unconditionally, which
 * is the same slow path, with the pool not merely empty but ABSENT (and a
 * session the runtime refuses at open, RFC-0113 D5). So this
 * guard forecloses nothing, which is the thing issue 1015's first fix got
 * wrong about issue 1033's pools.
 *
 * Below the `#ifndef`, never above: an undefined identifier reads as 0 in
 * `#if`, so a guard above its own default fires on every build (issue 1167). */
#if NROS_RMW_UORB_PX4_MAX_CALLBACKS < 1
#error "NROS_RMW_UORB_PX4_MAX_CALLBACKS must be >= 1: ISO C++ forbids a zero-size array (1131)"
#endif

// One callback per registration, called by the broker IN THE PUBLISHER'S
// CONTEXT (`uORB::SubscriptionCallback::call()`), exactly where PX4's own
// `SubscriptionCallbackWorkItem` calls `ScheduleNow()`.
//
// phase-521 W8 (d), issue 2218 — this used to be a PX4 `WorkItem` on
// `lp_default` owning a `SubscriptionCallbackWorkItem` pointed at itself, so
// every arrival was a hop through the LOWEST-priority work queue before the
// subscriber's flag flipped: a `rate_ctrl` sched task's arrival waited behind
// `lp_default`'s backlog. Now the callback runs where the broker delivers,
// and it is short by contract (an atomic store, the executor's wake, and the
// owning sched task's `ScheduleNow()`), so there is no queue to wait in.
class CallbackSlotImpl final : public uORB::SubscriptionCallback {
public:
    CallbackSlotImpl(const orb_metadata *meta, uint8_t instance, int handle_in,
                     nros_orb_callback_t cb, void *arg)
        : uORB::SubscriptionCallback(meta, 0, instance),
          sub_handle(handle_in), callback(cb), user_arg(arg) {}

    void call() override {
        if (callback != nullptr) {
            callback(user_arg);
        }
    }

    int handle() const { return sub_handle; }

private:
    int                 sub_handle;
    nros_orb_callback_t callback;
    void               *user_arg;
};

// Lazy-constructed pool: a slot is constructed at install (the callback
// needs its topic) and destroyed at uninstall, so a free slot holds no
// broker registration.
struct Slot {
    alignas(CallbackSlotImpl) unsigned char storage[sizeof(CallbackSlotImpl)]{};
    bool constructed = false;

    CallbackSlotImpl *adapter() {
        return reinterpret_cast<CallbackSlotImpl *>(storage);
    }
};

Slot g_pool[NROS_RMW_UORB_PX4_MAX_CALLBACKS];

Slot *find_free() {
    for (auto &slot : g_pool) {
        if (!slot.constructed) {
            return &slot;
        }
    }
    return nullptr;
}

Slot *find_by_handle(int handle) {
    for (auto &slot : g_pool) {
        if (slot.constructed && slot.adapter()->handle() == handle) {
            return &slot;
        }
    }
    return nullptr;
}

} // namespace

extern "C" {

int nros_orb_register_callback(const struct orb_metadata *meta,
                               uint8_t instance,
                               int handle,
                               nros_orb_callback_t cb,
                               void *arg) {
    if (meta == nullptr || cb == nullptr || handle < 0) {
        return -1;
    }
    Slot *slot = find_free();
    if (slot == nullptr) {
        PX4_ERR("nros uORB: callback pool full (NROS_RMW_UORB_PX4_MAX_CALLBACKS)");
        // Pool exhausted: the subscription is refused (subscriber.cpp).
        return -1;
    }
    CallbackSlotImpl *impl = new (slot->storage) CallbackSlotImpl(meta, instance, handle, cb, arg);
    slot->constructed = true;
    if (!impl->registerCallback()) {
        PX4_ERR("nros uORB: broker callback for `%s` did not register", meta->o_name);
        impl->~CallbackSlotImpl();
        slot->constructed = false;
        return -1;
    }
    return 0;
}

int nros_orb_unregister_callback(int handle) {
    Slot *slot = find_by_handle(handle);
    if (slot == nullptr) {
        return 0; // idempotent: not-found counts as success
    }
    // The destructor unregisters from the broker before the slot is reused.
    slot->adapter()->~CallbackSlotImpl();
    slot->constructed = false;
    return 0;
}

} // extern "C"
