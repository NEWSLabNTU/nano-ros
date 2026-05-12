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

int nros_orb_register_callback(int handle, nros_orb_callback_t cb, void *arg) {
    // The data-plane only has the handle at this point; metadata for
    // PX4's compositional API would need to be threaded through. Until
    // the runtime adds a (meta, instance) channel, decline the
    // registration. Subscriber falls back to the polling path.
    //
    // NOTE: leaving this as a stub on the strong-symbol side keeps the
    // K.4.5 SITL build honest — the glue compiles, links, and the
    // class hierarchy is exercised by the compiler; runtime push-wake
    // remains gated on the subscriber-side ABI extension.
    (void)handle;
    (void)cb;
    (void)arg;
    return -1;
}

int nros_orb_unregister_callback(int handle) {
    CallbackAdapter *slot = find_by_handle(handle);
    if (slot == nullptr) {
        return 0;
    }
    slot->uninstall();
    return 0;
}

// Internal hook for a future (meta, instance)-aware register ABI.
// Referenced from the C-side once `SubscriberState` carries
// metadata + instance through. Today the symbol just keeps the
// pool's install path linked and type-checked.
int nros_orb_register_callback_with_meta(const struct orb_metadata *meta,
                                         uint8_t instance,
                                         int handle,
                                         nros_orb_callback_t cb,
                                         void *arg) {
    if (meta == nullptr || cb == nullptr || handle < 0) {
        return -1;
    }
    CallbackAdapter *slot = find_free_or_construct();
    if (slot == nullptr) {
        return -1;
    }
    return slot->install(meta, instance, handle, cb, arg) ? 0 : -1;
}

} // extern "C"
