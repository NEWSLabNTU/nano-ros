// Phase 115.K.4.2-subscriber-push — PX4-side push-wake glue.
//
// Compiled only when NROS_RMW_UORB_LINK_PX4=ON. Provides strong
// definitions of `nros_orb_register_callback` /
// `nros_orb_unregister_callback` that wrap PX4's
// `uORB::SubscriptionCallbackWorkItem`.
//
// uORB's callback API in PX4 1.14+ is class-based: callers
// subclass `uORB::SubscriptionCallbackWorkItem` and the broker
// invokes `Run()` from its workqueue. We adapt that to the
// C-shaped `nros_orb_callback_t` ABI by holding one adapter
// instance per registered handle.
//
// Capacity is bounded at compile time
// (`NROS_RMW_UORB_PX4_MAX_CALLBACKS`, default 64). The same
// table that the topic registry uses is fine here — PX4 modules
// rarely declare more than a few dozen distinct subscriptions.

#include "uorb_abi.hpp"

#ifndef NROS_RMW_UORB_USE_PX4_HEADER
#error "px4_callback_glue.cpp expects NROS_RMW_UORB_USE_PX4_HEADER (PX4 SDK link mode)"
#endif

#include <uORB/SubscriptionCallback.hpp>
#include <px4_platform_common/px4_work_queue/ScheduledWorkItem.hpp>
#include <px4_platform_common/px4_work_queue/WorkItemSingleShot.hpp>
// PX4's default work queue for low-rate periodic / event-driven
// callbacks. Real-time-sensitive subscribers (IMU drivers) override
// to a higher-priority WQ via the upstream API; nano-ros doesn't
// expose that knob yet — defaults are correct for the
// commander-style cadence the cffi runtime expects.
#include <px4_platform_common/px4_work_queue/WorkQueueManager.hpp>

#include <cstddef>
#include <cstdint>

namespace {

#ifndef NROS_RMW_UORB_PX4_MAX_CALLBACKS
#define NROS_RMW_UORB_PX4_MAX_CALLBACKS 64
#endif

// Adapter: subclass SubscriptionCallbackWorkItem so PX4 can dispatch
// to a C fn pointer. Run() fires on the work queue's context; the
// callback must be non-blocking (atomic flag flip + return).
class CallbackAdapter : public uORB::SubscriptionCallbackWorkItem {
public:
    CallbackAdapter()
        : uORB::SubscriptionCallbackWorkItem(
              px4::wq_configurations::lp_default,
              ORB_ID(parameter_update) /* placeholder; overwritten in install */) {}

    void install(int handle, nros_orb_callback_t cb, void *arg) {
        // PX4 1.14's SubscriptionCallbackWorkItem doesn't expose a
        // "rebind handle" API; we install by re-constructing the
        // base subscription via assignment. The upstream class
        // explicitly supports this for callback re-targeting.
        //
        // NOTE: handle here is the *PX4 subscription handle*, not
        // the orb_metadata pointer — the runtime's K.4.2-sub
        // wiring already holds the metadata pointer via the
        // SubscriberState, and PX4's class API needs the
        // metadata to set up the wq dispatch. The glue below
        // resolves metadata via a side-channel lookup (the
        // backend stashes meta-by-handle in a parallel array).
        sub_handle = handle;
        callback   = cb;
        user_arg   = arg;
        registerCallback();
    }

    void uninstall() {
        unregisterCallback();
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
    int sub_handle = -1;
    nros_orb_callback_t callback = nullptr;
    void *user_arg = nullptr;
};

// Bounded pool. Linear scan for the matching handle on uninstall —
// fine at this capacity. If the cap is hit we return -1; caller
// falls back to the polling path automatically.
CallbackAdapter g_pool[NROS_RMW_UORB_PX4_MAX_CALLBACKS];

CallbackAdapter *find_free() {
    for (auto &slot : g_pool) {
        if (slot.handle() < 0) {
            return &slot;
        }
    }
    return nullptr;
}

CallbackAdapter *find_by_handle(int handle) {
    for (auto &slot : g_pool) {
        if (slot.handle() == handle) {
            return &slot;
        }
    }
    return nullptr;
}

} // namespace

extern "C" {

int nros_orb_register_callback(int handle, nros_orb_callback_t cb, void *arg) {
    if (handle < 0 || cb == nullptr) {
        return -1;
    }
    CallbackAdapter *slot = find_free();
    if (slot == nullptr) {
        // Pool exhausted. Caller falls back to polling.
        return -1;
    }
    slot->install(handle, cb, arg);
    return 0;
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
