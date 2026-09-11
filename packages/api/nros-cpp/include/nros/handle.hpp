// nros-cpp: the non-owning entity handle
// Freestanding C++ — no exceptions, no STL required

/**
 * @file handle.hpp
 * @ingroup grp_support
 * @brief `nros::Handle<T>` — what `X::SharedPtr` is on every target.
 */

#ifndef NROS_CPP_HANDLE_HPP
#define NROS_CPP_HANDLE_HPP

#include "nros/traits.hpp"

namespace nros {

/// A copyable, non-owning reference to an entity — RFC-0096 D2.
///
/// WHAT IT REPLACES, AND WHY NOTHING IS LOST
///
/// `X::SharedPtr` is the name ported rclcpp code writes, so it has to exist on
/// every target, and `std::shared_ptr` does not. What it has to DO was
/// measured rather than assumed (phase-442 W0), over this tree and the porting
/// corpus:
///
///   * handles are stored as members and one is passed by value
///     (`diagnostic_updater::Updater`'s constructor);
///   * there is no `weak_ptr`, no `.lock()`, no `use_count()`, no `reset()` and
///     no custom deleter anywhere outside our own headers;
///   * `rclcpp::Node::shared_from_this()` ALREADY returns
///     `std::shared_ptr<Node>(std::shared_ptr<void>(), this)` — the aliasing
///     constructor with an EMPTY owner, which observes and does not own.
///
/// So shared ownership was already fiction at the one site where it looked
/// real, and a reference count would be a mechanism with no reader. Building
/// one would be inventing work; RFC-0096 says so in as many words.
///
/// WHAT THAT COSTS A USER, STATED
///
/// Upstream's `shared_ptr` keeps the pointee alive; this does not. In this API
/// the entity is owned by the node or the executor arena and outlives everything
/// it is handed to, so the two behave identically — but a caller who stores a
/// handle past its entity's lifetime gets a dangling pointer where upstream
/// would have kept the object alive. That is the same envelope
/// `shared_from_this()` has documented since phase-427, now the whole type's.
///
/// One pointer, trivially copyable, no allocation, no atomics. `sizeof` is
/// `sizeof(void*)` on every target and follows no capability probe.
template <typename T> class Handle {
  public:
    using element_type = T;

    constexpr Handle() : p_(nullptr) {}
    /// Null, spelled the way a ported file spells it (`X::SharedPtr p = nullptr`).
    constexpr Handle(decltype(nullptr)) : p_(nullptr) {}
    explicit constexpr Handle(T* p) : p_(p) {}

    /// `Handle<T>` converts to `Handle<const T>`, the way `shared_ptr` does.
    /// Constrained rather than open so that an unrelated `Handle<U>` is a
    /// compile error at the call site instead of a surprise at the `static_cast`.
    template <typename U,
              typename tr::enable_if<tr::is_same<typename tr::remove_const<U>::type,
                                                 typename tr::remove_const<T>::type>::value,
                                     int>::type = 0>
    constexpr Handle(const Handle<U>& other) : p_(other.get()) {}

    constexpr T* get() const { return p_; }
    constexpr T* operator->() const { return p_; }
    constexpr T& operator*() const { return *p_; }

    /// `if (sub)` / `if (!sub)`. `explicit` so a handle does not silently
    /// convert to `bool` in arithmetic, which is what upstream's does too.
    explicit constexpr operator bool() const { return p_ != nullptr; }

    /// Release the reference. Does NOT destroy the entity — nothing here owns
    /// one. Present because ported code writes `timer_.reset()` to mean "stop
    /// referring to it"; where it meant "destroy it", the entity's own
    /// lifetime verb (`Timer::cancel()`) is what that code wants, and this
    /// leaves the object running rather than pretending otherwise.
    void reset() { p_ = nullptr; }

  private:
    T* p_;
};

template <typename T, typename U>
constexpr bool operator==(const Handle<T>& a, const Handle<U>& b) {
    return a.get() == b.get();
}
template <typename T, typename U>
constexpr bool operator!=(const Handle<T>& a, const Handle<U>& b) {
    return a.get() != b.get();
}
template <typename T> constexpr bool operator==(const Handle<T>& a, decltype(nullptr)) {
    return a.get() == nullptr;
}
template <typename T> constexpr bool operator!=(const Handle<T>& a, decltype(nullptr)) {
    return a.get() != nullptr;
}
template <typename T> constexpr bool operator==(decltype(nullptr), const Handle<T>& a) {
    return a.get() == nullptr;
}
template <typename T> constexpr bool operator!=(decltype(nullptr), const Handle<T>& a) {
    return a.get() != nullptr;
}

} // namespace nros

#endif // NROS_CPP_HANDLE_HPP
