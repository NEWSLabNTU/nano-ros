// nros-cpp: plumbing for upstream rclcpp_action's goal-handle shapes
// Freestanding C++ — no exceptions, no STL required

/**
 * @file upstream.hpp
 * @ingroup grp_action
 * @brief Fixed-layout carriers for the HOSTED-only `rclcpp_action` drop-ins
 *        (issue 2015).
 *
 * Upstream's action API hands goals around as `std::shared_ptr` goal handles
 * and takes its callbacks as `std::function`s. A hosted build (`NROS_CPP_STD`)
 * takes both as drop-ins; a freestanding one has no allocator for either and
 * refuses them (`NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE`).
 *
 * The objects that carry that state — `Client<A>`, `Server<A>`,
 * `Client<A>::SendGoalOptions`, `ServerGoalHandle<A>` — must not change
 * `sizeof` with the flag (`check-cpp-capability-layout`: two TUs of one image
 * may disagree about it). So the hosted state lives on the heap, behind members
 * that are one or two pointers wide in EVERY configuration and simply stay null
 * on a freestanding build.
 */

#ifndef NROS_RCLCPP_ACTION_DETAIL_UPSTREAM_HPP
#define NROS_RCLCPP_ACTION_DETAIL_UPSTREAM_HPP

#include "rclcpp/detail/std_detect.hpp" // NROS_CPP_HAS_SHARED_PTR, <memory>, <functional>
#include "rclcpp/detail/traits.hpp"
#include "rclcpp/logging.hpp"              // NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE, detail::refuse
#include "rclcpp/detail/nros_cpp_ffi.h"    // nros_cpp_spin_once / nros_cpp_time_ns (issue 2027)
#include "rclcpp/detail/driven_future.hpp" // PumpCell, DrivenSharedFuture (issue 2037)

namespace rclcpp_action {
namespace detail {

/// Owns one heap object of a type only hosted code names. Two pointers in
/// every configuration; never allocated on a freestanding build.
class HostedBox {
  public:
    HostedBox() : p_(nullptr), del_(nullptr) {}
    ~HostedBox() { reset(); }
    HostedBox(HostedBox&& o) : p_(o.p_), del_(o.del_) {
        o.p_ = nullptr;
        o.del_ = nullptr;
    }
    HostedBox& operator=(HostedBox&& o) {
        if (this != &o) {
            reset();
            p_ = o.p_;
            del_ = o.del_;
            o.p_ = nullptr;
            o.del_ = nullptr;
        }
        return *this;
    }
    HostedBox(const HostedBox&) = delete;
    HostedBox& operator=(const HostedBox&) = delete;

    /// Take `p`, destroyed by `del` when this box is reset or destroyed.
    void reset(void* p = nullptr, void (*del)(void*) = nullptr) {
        if (del_ != nullptr) del_(p_);
        p_ = p;
        del_ = del;
    }
    template <typename T> T* as() const { return static_cast<T*>(p_); }

  private:
    void* p_;
    void (*del_)(void*);
};

/// What a goal-handle `SharedPtr` (and the other upstream types built on
/// `std::shared_ptr`) is on a freestanding build: naming it compiles, and any
/// use that needs the type complete — a by-value parameter in a function
/// definition, a member, a local — fails with the refusal.
template <typename T> struct Refused {
    static_assert(::rclcpp::detail::refuse<T>::value, NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE);
};

/// A COPYABLE callable slot standing in for upstream's `std::function` fields
/// (`SendGoalOptions::goal_response_callback` …). One pointer in every
/// configuration: hosted, it owns a heap `std::function<Sig>`; freestanding it
/// is always empty and assigning a callable to it is refused.
template <typename Sig> class UpstreamCallback;

template <typename R, typename... A> class UpstreamCallback<R(A...)> {
  public:
    UpstreamCallback() : fn_(nullptr) {}
    UpstreamCallback(decltype(nullptr)) : fn_(nullptr) {}
    ~UpstreamCallback() { clear(); }
    UpstreamCallback(const UpstreamCallback& o) : fn_(nullptr) { copy_from(o); }
    UpstreamCallback& operator=(const UpstreamCallback& o) {
        // Not `&o`: a unary `&` on this type looks up `operator&` by ADL, whose
        // associated classes include every type in `Sig` -- and on a
        // freestanding build those are `Refused<...>`, so the lookup completes
        // one and fires its refusal on a plain `options` copy that assigns no
        // upstream field (`Client::set_callbacks`). No `<memory>` freestanding,
        // so the builtin `std::addressof` is built on.
        if (this != __builtin_addressof(o)) {
            clear();
            copy_from(o);
        }
        return *this;
    }
    UpstreamCallback& operator=(decltype(nullptr)) {
        clear();
        return *this;
    }

    /// Store `f` (a `std::bind` result, a lambda, a `std::function`).
    template <
        typename F,
        typename ::rclcpp::tr::enable_if<
            !::rclcpp::tr::is_same<typename ::rclcpp::tr::decay<F>::type, UpstreamCallback>::value,
            int>::type = 0>
    UpstreamCallback& operator=(F&& f) {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
        clear();
        fn_ = new ::std::function<R(A...)>(static_cast<F&&>(f));
#else
        static_assert(::rclcpp::detail::refuse<F>::value, NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE);
        (void)f;
#endif
        return *this;
    }

    explicit operator bool() const { return fn_ != nullptr; }

#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    R operator()(A... a) const { return (*get())(static_cast<A&&>(a)...); }
#endif

  private:
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    ::std::function<R(A...)>* get() const { return static_cast<::std::function<R(A...)>*>(fn_); }
#endif
    void clear() {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
        delete get();
#endif
        fn_ = nullptr;
    }
    void copy_from(const UpstreamCallback& o) {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
        if (o.fn_ != nullptr) fn_ = new ::std::function<R(A...)>(*o.get());
#else
        (void)o;
#endif
    }

    void* fn_;
};

/// Issue 2027 — the driven `std::shared_future` and the cell it pumps. They
/// moved to `rclcpp/detail/driven_future.hpp` (issue 2037), because a service
/// request's `share()` returns one too; these are the names this header's
/// users always spelled.
using PumpCell = ::rclcpp::detail::PumpCell;
#ifdef NROS_CPP_HAS_STD_PROMISE // hosted-family: action-interop
template <typename T> using DrivenSharedFuture = ::rclcpp::detail::DrivenSharedFuture<T>;
#endif

} // namespace detail
} // namespace rclcpp_action

#endif // NROS_RCLCPP_ACTION_DETAIL_UPSTREAM_HPP
