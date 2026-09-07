// Phase 115.K.4.2-subscriber-push — PX4-side push-wake glue.
//
// Compiled only when NROS_RMW_UORB_BUILD_PX4_GLUE=ON (which itself
// implies NROS_RMW_UORB_LINK_PX4=ON). Provides strong definitions
// of `nros_orb_register_callback` / `nros_orb_unregister_callback`
// that wrap PX4's `uORB::SubscriptionCallbackWorkItem`.
//
// PX4's push-wake API in 1.14+ is **compositional**, not
// subclass-based:
//   * `uORB::SubscriptionCallbackWorkItem` holds a pointer to a
//     `px4::WorkItem` (separate object) and calls `ScheduleNow()`
//     on it when the broker publishes.
//   * The `WorkItem`'s `Run()` override does the actual work,
//     dispatched on a worker thread owned by the configured WQ.
//
// We expose a C ABI shaped like
// `int register(handle, fn, arg)`, so we adapt the compositional
// API by creating one **adapter object per registration** that:
//   1. Subclasses `px4::WorkItem` (so its `Run()` invokes our C fn).
//   2. Owns a placement-new'd `SubscriptionCallbackWorkItem` that
//      points back at itself.
//
// Capacity is bounded at compile time
// (`NROS_RMW_UORB_PX4_MAX_CALLBACKS`, default 64). Adapters are
// stored in a fixed pool with `alignas` storage so we can build
// them lazily (PX4's WQs aren't running yet when firmware-level
// static globals construct).

#include "uorb_abi.hpp"

#ifndef NROS_RMW_UORB_USE_PX4_HEADER
#error "px4_callback_glue.cpp expects NROS_RMW_UORB_USE_PX4_HEADER (PX4 SDK link mode)"
#endif

#include <uORB/SubscriptionCallback.hpp>
#include <px4_platform_common/px4_work_queue/WorkItem.hpp>
#include <px4_platform_common/px4_work_queue/WorkQueueManager.hpp>

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
 * Zero looked arguable, and the runtime half of the argument is sound: at 0
 * both range-`for`s over `g_pool` simply do not execute, `find_free_or_construct`
 * returns NULL, `nros_orb_register_callback` returns -1, and `subscriber.cpp`
 * treats that as its DOCUMENTED slow path — it pins `ready` true and polls
 * `orb_check` every time, "same behaviour the pre-push-wake K.4.2 build had".
 * That is not issue 1015's silence: data still flows. So zero does not break
 * the runtime.
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
 * `callback_default.cpp`'s weak stubs — those return -1 unconditionally, which
 * is the same slow path, with the pool not merely empty but ABSENT. So this
 * guard forecloses nothing, which is the thing issue 1015's first fix got
 * wrong about issue 1033's pools.
 *
 * Below the `#ifndef`, never above: an undefined identifier reads as 0 in
 * `#if`, so a guard above its own default fires on every build (issue 1167). */
#if NROS_RMW_UORB_PX4_MAX_CALLBACKS < 1
#error "NROS_RMW_UORB_PX4_MAX_CALLBACKS must be >= 1: ISO C++ forbids a zero-size array (1131)"
#endif

// One adapter per registration. The WorkItem half handles the WQ
// dispatch; the SubscriptionCallbackWorkItem half is constructed in
// `install()` (after PX4's WQs have come up) and points back at
// the WorkItem half via `this`.
class CallbackAdapter : public px4::WorkItem {
public:
    CallbackAdapter()
        : px4::WorkItem("nros_orb_cb", px4::wq_configurations::lp_default) {}

    ~CallbackAdapter() override = default;

    bool install(const orb_metadata *meta, uint8_t instance,
                 int handle_in, nros_orb_callback_t cb, void *arg) {
        new (sub_cb_storage) uORB::SubscriptionCallbackWorkItem(this, meta, instance);
        sub_cb_constructed = true;
        sub_handle         = handle_in;
        callback           = cb;
        user_arg           = arg;
        return sub_cb()->registerCallback();
    }

    void uninstall() {
        if (sub_cb_constructed) {
            sub_cb()->unregisterCallback();
            sub_cb()->~SubscriptionCallbackWorkItem();
            sub_cb_constructed = false;
        }
        sub_handle = -1;
        callback   = nullptr;
        user_arg   = nullptr;
    }

    int handle() const { return sub_handle; }

protected:
    void Run() override {
        if (callback != nullptr) {
            callback(user_arg);
        }
    }

private:
    alignas(uORB::SubscriptionCallbackWorkItem) unsigned char
        sub_cb_storage[sizeof(uORB::SubscriptionCallbackWorkItem)]{};
    bool                sub_cb_constructed = false;
    int                 sub_handle         = -1;
    nros_orb_callback_t callback           = nullptr;
    void               *user_arg           = nullptr;

    uORB::SubscriptionCallbackWorkItem *sub_cb() {
        return reinterpret_cast<uORB::SubscriptionCallbackWorkItem *>(sub_cb_storage);
    }
};

// Lazy-constructed pool. We can't put CallbackAdapter directly in a
// global array because its `WorkItem` base constructor runs at
// firmware-boot static-init time — well before the WQ manager is up.
// Each slot tracks construction state; first install() in that slot
// placement-new's the adapter.
struct Slot {
    alignas(CallbackAdapter) unsigned char storage[sizeof(CallbackAdapter)]{};
    bool constructed = false;

    CallbackAdapter *adapter() {
        return reinterpret_cast<CallbackAdapter *>(storage);
    }
};

Slot g_pool[NROS_RMW_UORB_PX4_MAX_CALLBACKS];

CallbackAdapter *find_free_or_construct() {
    for (auto &slot : g_pool) {
        if (!slot.constructed) {
            new (slot.storage) CallbackAdapter();
            slot.constructed = true;
            return slot.adapter();
        }
        if (slot.adapter()->handle() < 0) {
            return slot.adapter();
        }
    }
    return nullptr;
}

CallbackAdapter *find_by_handle(int handle) {
    for (auto &slot : g_pool) {
        if (slot.constructed && slot.adapter()->handle() == handle) {
            return slot.adapter();
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
    CallbackAdapter *slot = find_free_or_construct();
    if (slot == nullptr) {
        // Pool exhausted. Caller falls back to polling.
        return -1;
    }
    return slot->install(meta, instance, handle, cb, arg) ? 0 : -1;
}

int nros_orb_unregister_callback(int handle) {
    CallbackAdapter *slot = find_by_handle(handle);
    if (slot == nullptr) {
        return 0; // idempotent: not-found counts as success
    }
    slot->uninstall();
    return 0;
}

} // extern "C"
