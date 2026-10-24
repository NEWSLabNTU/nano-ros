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
#include "rclcpp/logging.hpp"           // NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE, detail::refuse
#include "rclcpp/detail/nros_cpp_ffi.h" // nros_cpp_spin_once / nros_cpp_time_ns (issue 2027)

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
        if (this != &o) {
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

/// Issue 2027 — what a `DrivenSharedFuture` drives: one pass of its CLIENT's
/// own dispatch (`ctx`, null once the client is gone or moved away) and the
/// executor that client was created on. Shared between the client and every
/// future it handed out, so a future never names a dead client.
struct PumpCell {
    void (*fn)(void*) = nullptr;
    void* ctx = nullptr;
    void* executor = nullptr;
};

#ifdef NROS_CPP_HAS_STD_PROMISE // hosted-family: action-interop
/// Issue 2027 — the `std::shared_future` upstream's `async_get_result` /
/// `async_cancel_goal` return, IS-A `std::shared_future<T>`.
///
/// A plain `std::shared_future` waits for another thread to fulfil it, and
/// there is none here (RFC-0021): its value arrives from the client's
/// dispatch, during a spin. So this one drives that dispatch while it waits:
/// `is_ready()` (what `rclcpp::spin_until_future_complete` polls) runs one
/// pass of the client's own dispatch first, and `wait_for` / `get` spin the
/// client's executor in 10 ms slices, as `rclcpp::Future::wait_for` does.
/// Converted to a bare `std::shared_future<T>` it still completes, but only
/// when something else drives the client (`poll()`, a spin, another future).
template <typename T> class DrivenSharedFuture : public ::std::shared_future<T> {
  public:
    DrivenSharedFuture() {}
    DrivenSharedFuture(::std::shared_future<T> f, ::std::shared_ptr<PumpCell> pump)
        : ::std::shared_future<T>(static_cast<::std::shared_future<T>&&>(f)),
          pump_(static_cast<::std::shared_ptr<PumpCell>&&>(pump)) {}

    /// The value has arrived. Drives one pass of the client's dispatch first.
    bool is_ready() const {
        if (!this->valid()) return false;
        if (settled()) return true;
        if (pump_ && pump_->fn != nullptr && pump_->ctx != nullptr) pump_->fn(pump_->ctx);
        return settled();
    }

    /// `std::shared_future::wait_for`, DRIVEN: spins the client's executor in
    /// slices of at most 10 ms until the value arrives or the budget runs out.
    template <typename Rep, typename Period>
    ::std::future_status wait_for(::std::chrono::duration<Rep, Period> timeout) const {
        if (!this->valid()) return ::std::future_status::timeout;
        if (is_ready()) return ::std::future_status::ready;
        const int64_t ms = ::rclcpp::detail::chrono_wait_budget_ms(timeout);
        void* exec = pump_ ? pump_->executor : nullptr;
        if (ms <= 0 || exec == nullptr) return ::std::future_status::timeout;
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(ms) * 1000000ULL;
        while (nros_cpp_time_ns() - start_ns < budget_ns) {
            (void)nros_cpp_spin_once(exec, 10);
            if (is_ready()) return ::std::future_status::ready;
        }
        return is_ready() ? ::std::future_status::ready : ::std::future_status::timeout;
    }

    /// `std::shared_future::get`, DRIVEN: spins the client's executor until
    /// the value arrives (unbounded, like `spin_until_future_complete`
    /// without a timeout). Call it after that, as the stock examples do. On a
    /// future whose client is gone and whose value never came it does not
    /// wait: the base `get()` then reports the broken promise.
    const T& get() const {
        while (this->valid() && !is_ready() && pump_ && pump_->ctx != nullptr &&
               pump_->executor != nullptr) {
            (void)nros_cpp_spin_once(pump_->executor, 10);
        }
        return ::std::shared_future<T>::get();
    }

  private:
    bool settled() const {
        return ::std::shared_future<T>::wait_for(::std::chrono::seconds(0)) ==
               ::std::future_status::ready;
    }
    ::std::shared_ptr<PumpCell> pump_;
};
#endif

} // namespace detail
} // namespace rclcpp_action

#endif // NROS_RCLCPP_ACTION_DETAIL_UPSTREAM_HPP
