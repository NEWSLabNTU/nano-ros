// nros-cpp: the dispatch subscription's handle
// Freestanding C++ — no exceptions, no STL required

/**
 * @file subscription_handle.hpp
 * @ingroup grp_pubsub
 * @brief `nros::SubscriptionHandle<M>` — what `Subscription<M>::SharedPtr` is.
 */

#ifndef NROS_CPP_SUBSCRIPTION_HANDLE_HPP
#define NROS_CPP_SUBSCRIPTION_HANDLE_HPP

#include <cstddef>

namespace nros {

/// A registered dispatch subscription — phase-456 W2.
///
/// WHAT IT IS, AND WHY IT IS NOT A POINTER TO A `Subscription<M>`
///
/// A subscription created with a CALLBACK is owned by the Rust executor arena:
/// the arena holds the `RmwSubscriber`, the rx buffer, the callback and (since
/// phase-456 W1) the callback's capture. Nothing of the caller's is referenced
/// after registration, so there is no C++ object to point at and none is made.
///
/// `nros.hpp` used to hand back a `std::shared_ptr<Subscription<M>>` aliasing
/// into a heap cell, and said in its own comment what that pointer was:
///
///     WHAT THE RETURNED POINTER IS: a keep-alive ... The executor owns the
///     real subscriber, so `sub->take(msg)` on it answers `NotInitialized` --
///     the sample went to your callback.
///
/// So the object carried `take()`, `take_serialized()`, `take_validated()`,
/// `take_sequence()` and `borrow()`, every one of them present on the type a
/// porter is handed and every one guaranteed to fail. This type is the same
/// keep-alive with the failing half removed.
///
/// WHAT IT DELIBERATELY DOES NOT HAVE
///
/// No `operator->`, because there is nothing to dereference. A ported file that
/// calls a method on its subscription handle gets a compile error naming the
/// handle, which is the mechanical edit RFC-0089 asks for — and a better
/// outcome than a call that compiles and returns `NotInitialized` at runtime.
/// Measured (phase-456 W2): the ported corpus calls NOTHING on it. It is
/// stored and dropped, which is exactly what a keep-alive is for.
///
/// No unregister, no `cancel()`. The executor arena has no removal path — the
/// registration lives as long as the executor does. Offering a verb that
/// cannot be implemented is the defect this type exists to remove, so it is not
/// offered.
///
/// WHAT IT COSTS
///
/// Two words, trivially copyable, no allocation. `Subscription<M>` on the
/// dispatch path used to be 888 bytes of which `storage_` was unused, plus a
/// heap cell.
template <typename M> class SubscriptionHandle {
  public:
    /// The message type, for the same reason `std::shared_ptr` exposes one.
    using message_type = M;

    constexpr SubscriptionHandle() : executor_(nullptr), handle_id_(0) {}
    /// Null, spelled the way a ported file spells it
    /// (`Subscription<M>::SharedPtr sub_ = nullptr;`).
    constexpr SubscriptionHandle(decltype(nullptr)) : executor_(nullptr), handle_id_(0) {}
    constexpr SubscriptionHandle(void* executor, size_t handle_id)
        : executor_(executor), handle_id_(handle_id) {}

    /// `if (sub_)` / `if (!sub_)`. A default-constructed handle is empty; one
    /// from a successful `create_subscription` never is.
    explicit constexpr operator bool() const { return executor_ != nullptr; }

    /// The executor arena slot this registration occupies. Present for
    /// introspection and for tests that assert a registration happened; there
    /// is nothing to do with it through this API today.
    constexpr size_t handle_id() const { return handle_id_; }

    /// Stop referring to the registration. Does NOT unregister it — the arena
    /// has no removal path, and the callback goes on firing. Present because
    /// ported code writes `sub_.reset()` meaning "I am done with this handle",
    /// and that is exactly what this does.
    void reset() {
        executor_ = nullptr;
        handle_id_ = 0;
    }

  private:
    void* executor_;
    size_t handle_id_;
};

template <typename M>
constexpr bool operator==(const SubscriptionHandle<M>& a, const SubscriptionHandle<M>& b) {
    return a.handle_id() == b.handle_id() && static_cast<bool>(a) == static_cast<bool>(b);
}
template <typename M>
constexpr bool operator!=(const SubscriptionHandle<M>& a, const SubscriptionHandle<M>& b) {
    return !(a == b);
}
template <typename M> constexpr bool operator==(const SubscriptionHandle<M>& a, decltype(nullptr)) {
    return !static_cast<bool>(a);
}
template <typename M> constexpr bool operator!=(const SubscriptionHandle<M>& a, decltype(nullptr)) {
    return static_cast<bool>(a);
}
template <typename M> constexpr bool operator==(decltype(nullptr), const SubscriptionHandle<M>& a) {
    return !static_cast<bool>(a);
}
template <typename M> constexpr bool operator!=(decltype(nullptr), const SubscriptionHandle<M>& a) {
    return static_cast<bool>(a);
}

} // namespace nros

#endif // NROS_CPP_SUBSCRIPTION_HANDLE_HPP
