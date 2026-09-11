// nros-cpp: the owning entity holder
// Freestanding C++ — no exceptions, no STL required

/**
 * @file owned.hpp
 * @ingroup grp_support
 * @brief `nros::Owned<T>` — what an entity's `X::SharedPtr` is on every target.
 */

#ifndef NROS_CPP_OWNED_HPP
#define NROS_CPP_OWNED_HPP

#include "nros/traits.hpp"

namespace nros {

/// Owns an entity BY VALUE and speaks pointer — phase-442 W8.
///
/// THIS IS THE EXCEPTION, NOT THE RULE. READ THIS FIRST.
///
/// An entity that the executor DISPATCHES to — a subscription, a service, an
/// action, a timer — is owned by the Rust arena, not by C++, and its
/// `X::SharedPtr` is a `nros::Handle` over `{executor, handle_id}`. That is
/// what `Timer` already is, and the ABI has the entry point for it:
/// `nros_cpp_subscription_register(..., out_handle_id)`, whose own doc says
/// "arena (rclcpp dispatch model), as opposed to the poll-style
/// `nros_cpp_subscription_create` above. The arena owns the subscriber."
///
/// `Owned<T>` is for the entities with NO dispatch, where no arena slot exists
/// and the C++ object genuinely is the entity: **publishers**, and the
/// poll-style forms of the others. It is one kind and a fallback, not the
/// shape of the API.
///
/// WHY THAT DISTINCTION IS THE DESIGN AND NOT AN IMPLEMENTATION DETAIL
///
/// The C/C++ API is a thin wrapper over the Rust API; entity lifetime is a Rust
/// data structure's. Where that holds, C++ should hold a handle and nothing
/// else — anything more is a second copy of state the arena already keeps.
/// `SubBufferedRawCEntry` holds the `RmwSubscriber`, the rx buffer, the
/// callback and its context, all of it; a C++ `Subscription<M>` carrying 888
/// bytes of its own storage on that path is duplication that accumulated, not
/// a decision.
///
/// WHY AN OWNING VALUE, WHERE IT DOES APPLY
///
/// `X::SharedPtr` is the name a ported file writes for an entity it keeps as a
/// member. Upstream's is a `std::shared_ptr`, which needs an allocator and a
/// control block. For a publisher there is no arena slot to point at, so the
/// entity has to live somewhere C++ can name — and it can live in the member
/// the ported file already declares, because the RMW handle RELOCATES. Every
/// caller-storage entity has a `relocate` entry point, and the ABI states the
/// contract:
///
///     Subscriptions are pull-based (`take_serialized`) and register nothing
///     externally that references the storage address -- relocation is a
///     straight `ptr::read` + `ptr::write`.
///
/// ```cpp
/// class MinimalPublisher : public rclcpp::Node {
///     rclcpp::Publisher<Msg>::SharedPtr pub_;   // no dispatch, so the entity IS here
///   public:
///     MinimalPublisher() { pub_ = this->create_publisher<Msg>("chatter", 10); }
///     void tick() { pub_->publish(msg); }
/// };
/// ```
///
/// THE LIMIT, AND WHY IT IS NOT A GAP
///
/// Relocatability belongs to the RMW handle, not to the C++ object. Where the
/// runtime has been handed a pointer to the OBJECT, moving it leaves that
/// pointer stale, and the tree says so in the move constructor of the type it
/// applies to (`service.hpp`):
///
///     A callback-style service must NOT be moved after register -- the arena
///     holds `this` as the trampoline context (Phase 189.M3.3.e); the move only
///     transfers bookkeeping and leaves that pointer stale, so don't.
///
/// That is not a hole in `Owned<T>`; it is the boundary of where `Owned<T>`
/// belongs. An entity the runtime knows by address is a DISPATCH entity, and a
/// dispatch entity takes the handle shape above, at which point there is no C++
/// object to move and the hazard has no subject.
///
/// WHAT IT COSTS, MEASURED
///
/// Two relocations per creation on C++14 and C++17 alike — the temporary into
/// the holder, the holder into the member. Elision does not remove the second,
/// which is an assignment rather than an initialisation. Each is the
/// `ptr::read` + `ptr::write` the ABI documents, at construction time only;
/// nothing on the publish path moves.
///
/// HOW IT DIFFERS FROM `std::shared_ptr`, STATED
///
/// It is MOVE-ONLY, because a publisher is. Upstream's handle copies. The W0
/// census found no entity handle copied anywhere in this tree or the porting
/// corpus, so this costs nothing measured — and it applies only to the kinds
/// listed above, because a dispatch entity's handle is `nros::Handle`, which
/// copies.
///
/// There is also no `use_count()`, no `weak_ptr`, and no custom deleter. The
/// entity's destructor runs when the holder does, which for the ported pattern
/// is when the node does — the same observable lifetime upstream gives it.
template <typename T> class Owned {
  public:
    using element_type = T;

    Owned() : value_(), live_(false) {}
    /// Null, spelled the way a ported file spells it (`X::SharedPtr p = nullptr`).
    Owned(decltype(nullptr)) : value_(), live_(false) {}

    /// Take ownership of `v`. Explicit so an entity does not silently become a
    /// handle at a call site that meant to keep the value.
    explicit Owned(T&& v) : value_(static_cast<T&&>(v)), live_(true) {}

    Owned(Owned&& other) : value_(static_cast<T&&>(other.value_)), live_(other.live_) {
        other.live_ = false;
    }

    Owned& operator=(Owned&& other) {
        if (this != &other) {
            value_ = static_cast<T&&>(other.value_);
            live_ = other.live_;
            other.live_ = false;
        }
        return *this;
    }

    Owned(const Owned&) = delete;
    Owned& operator=(const Owned&) = delete;

    T* operator->() { return &value_; }
    const T* operator->() const { return &value_; }
    T& operator*() { return value_; }
    const T& operator*() const { return value_; }

    /// The entity's address, or null when empty. Valid until this holder is
    /// moved from or destroyed — which is the one thing a caller must not
    /// assume about `std::shared_ptr`'s `get()`, so it is said here.
    T* get() { return live_ ? &value_ : nullptr; }
    const T* get() const { return live_ ? &value_ : nullptr; }

    /// `if (pub)` / `if (!pub)`.
    explicit operator bool() const { return live_; }

    /// Destroy the entity now rather than at scope end. `X::SharedPtr::reset()`
    /// in ported code means "drop my reference"; here there is only one
    /// reference, so dropping it destroys, which is what upstream does when it
    /// was the last one.
    void reset() {
        if (live_) {
            value_ = T();
            live_ = false;
        }
    }

  private:
    T value_;
    bool live_;
};

template <typename T> bool operator==(const Owned<T>& a, decltype(nullptr)) {
    return !static_cast<bool>(a);
}
template <typename T> bool operator!=(const Owned<T>& a, decltype(nullptr)) {
    return static_cast<bool>(a);
}
template <typename T> bool operator==(decltype(nullptr), const Owned<T>& a) {
    return !static_cast<bool>(a);
}
template <typename T> bool operator!=(decltype(nullptr), const Owned<T>& a) {
    return static_cast<bool>(a);
}

} // namespace nros

#endif // NROS_CPP_OWNED_HPP
