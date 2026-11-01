// nros-cpp: a std::shared_future that drives its producer while it waits
// Freestanding C++ — no exceptions, no STL required

/**
 * @file driven_future.hpp
 * @ingroup grp_executor
 * @brief `rclcpp::detail::DrivenSharedFuture` — the hosted `std::shared_future`
 *        whose waits drive the dispatch that fulfils it (issues 2027, 2037).
 *
 * Issue 2027 wrote it for `rclcpp_action::Client::async_get_result` /
 * `async_cancel_*`; issue 2037 moved it here, out of `rclcpp_action`, because
 * a service request's `FutureAndRequestId::share()` returns one too.
 */

#ifndef NROS_RCLCPP_DETAIL_DRIVEN_FUTURE_HPP
#define NROS_RCLCPP_DETAIL_DRIVEN_FUTURE_HPP

#include <cstdint>

#include "rclcpp/detail/std_detect.hpp" // NROS_CPP_HAS_STD_PROMISE, <future>, <memory>
#include "rclcpp/logging.hpp"           // chrono_wait_budget_ms
#include "rclcpp/detail/nros_cpp_ffi.h" // nros_cpp_spin_once / nros_cpp_time_ns

namespace rclcpp {
namespace detail {

/// Issue 2027 — what a `DrivenSharedFuture` drives: one pass of its CLIENT's
/// own dispatch (`ctx`, null once the client is gone or moved away) and the
/// executor that client was created on. Shared between the client and every
/// future it handed out, so a future never names a dead client.
struct PumpCell {
    void (*fn)(void*) = nullptr;
    void* ctx = nullptr;
    void* executor = nullptr;
};

#ifdef NROS_CPP_HAS_STD_PROMISE // hosted-family: shared-future-interop
/// Issue 2027 — the `std::shared_future` upstream's `async_get_result` /
/// `async_cancel_goal` return, IS-A `std::shared_future<T>`; issue 2037 —
/// and the `SharedFuture` a service request's `share()` returns.
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

    /// `std::shared_future::wait_for`, DRIVEN: spins the client's executor
    /// until the value arrives or the budget runs out, each spin parked until
    /// an arrival, the budget's end or the executor's next deadline.
    template <typename Rep, typename Period>
    ::std::future_status wait_for(::std::chrono::duration<Rep, Period> timeout) const {
        if (!this->valid()) return ::std::future_status::timeout;
        if (is_ready()) return ::std::future_status::ready;
        const int64_t ms = ::rclcpp::detail::chrono_wait_budget_ms(timeout);
        void* exec = pump_ ? pump_->executor : nullptr;
        if (ms <= 0 || exec == nullptr) return ::std::future_status::timeout;
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(ms) * 1000000ULL;
        while (true) {
            const uint64_t spent_ns = nros_cpp_time_ns() - start_ns;
            if (spent_ns >= budget_ns) break;
            // phase-521 W6 — park until the reply, the end of the budget or the
            // executor's next deadline, never a fixed 10 ms slice.
            const uint64_t left_ms = (budget_ns - spent_ns + 999999ULL) / 1000000ULL;
            const int32_t entry_ms = nros_cpp_entry_spin_budget_ms(exec);
            const int32_t slice = left_ms < static_cast<uint64_t>(entry_ms)
                                      ? static_cast<int32_t>(left_ms)
                                      : entry_ms;
            (void)nros_cpp_spin_once(exec, slice);
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
            (void)nros_cpp_spin_once(pump_->executor,
                                     nros_cpp_entry_spin_budget_ms(pump_->executor));
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
} // namespace rclcpp

#endif // NROS_RCLCPP_DETAIL_DRIVEN_FUTURE_HPP
