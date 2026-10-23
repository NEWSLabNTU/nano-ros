// nros-cpp: Future<T> -- single-shot deferred result
// Freestanding C++ -- no exceptions, no STL required

/**
 * @file future.hpp
 * @ingroup grp_executor
 * @brief `rclcpp::Future<T>` — single-shot deferred result.
 */

#ifndef NROS_RCLCPP_FUTURE_HPP
#define NROS_RCLCPP_FUTURE_HPP

#include <cstdint>
#include <cstddef>

#include "rclcpp/result.hpp"
#include "rclcpp/size_bound.hpp" // rclcpp::rx_buffer_capacity<T> — the receive-buffer size
// issue 1996 — `refuse<T>`, `say_refused` and the `NROS_RCLCPP_REFUSE_FUTURE_GET*`
// messages; it also brings `std_detect.hpp`, which defines the capability
// macros `get()` and `wait_for()` are gated on.
#include "rclcpp/logging.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_spin_once(void* handle, int32_t timeout_ms);
/// Monotonic time in nanoseconds. Used by Future::wait() to budget its
/// spin loop by wall-clock, not iteration count (Phase 89.2).
uint64_t nros_cpp_time_ns(void);
}

// phase-428 (RFC-0089): the entity types that reach into this one's private
// constructor are DEFINED in the upstream namespace now, so the friend
// declarations below name them there. A qualified friend cannot introduce a
// name, hence these declarations.
namespace rclcpp {
template <typename M> class Subscription;
} // namespace rclcpp

namespace rclcpp_action {
template <typename A> class Client;
} // namespace rclcpp_action

namespace rclcpp {

// phase-456 W9 — `send_request` and the `Future` it hands back moved off
// `rclcpp::Client<S>` onto `rclcpp::PollClient<S>`, the half that owns the
// `RmwServiceClient` this future takes from. The friend list FOLLOWS the method:
// the poll client is declared here, and `rclcpp::Client<S>` left both this
// declaration and the friend list below, because a friendship nothing uses is a
// claim about the code that is not true.
template <typename S> class PollClient;

/// Single-shot deferred result for request/response operations.
///
/// Returned by PollClient<S>::send_request (phase-456 W9; it was
/// `Client<S>::send_request` before the split). The future is consumed
/// when the result is taken -- move-only, single-shot.
///
/// Usage:
/// ```cpp
/// auto fut = client.send_request(req);
/// ResponseType resp;
/// RCLCPP_TRY(fut.wait(executor.handle(), 5000, resp));
/// ```
///
/// @tparam Cap  Bytes the RECEIVE buffer holds. Defaults to `T`'s own derived
///              bound where it has one, and to the legacy
///              `T::SERIALIZED_SIZE_MAX` estimate where it does not. The
///              `_sized` escape hatches are `Client::send_request_sized` and
///              `ActionClient::get_result_future_sized`, which hand back a
///              `Future<T, N>` (issue 0964). This buffer IS the capacity:
///              under-sizing it truncates the reply.
template <typename T, size_t Cap = ::rclcpp::rx_buffer_capacity<T>::value> class Future {
  public:
    /// Check if the result has arrived (non-blocking).
    bool is_ready() {
        if (slot_ < 0 || !take_fn_) return false;
        if (ready_) return true;
        uint8_t buf[Cap];
        size_t len = 0;
        nros_cpp_ret_t ret = take_fn_(client_storage_, buf, sizeof(buf), &len);
        if (ret == 0 && len > 0) {
            ready_ = true;
            cached_len_ = len < sizeof(cached_buf_) ? len : sizeof(cached_buf_);
            for (size_t i = 0; i < cached_len_; ++i)
                cached_buf_[i] = buf[i];
            return true;
        }
        return false;
    }

    /// Take the result if ready, consuming the future.
    ///
    /// @param out  Output object (filled on success).
    /// @return Result::success() on ready, ErrorCode::Error if not ready or failed.
    Result try_take(T& out) {
        if (slot_ < 0) return Result(ErrorCode::Error);
        if (!ready_ && !is_ready()) return Result(ErrorCode::Error);
        slot_ = -1; // consume
        if (T::ffi_deserialize(cached_buf_, cached_len_, &out) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result::success();
    }

    /// Block until the result arrives, spinning the executor.
    ///
    /// @param executor_handle  Raw executor handle (Executor::handle() or global).
    /// @param timeout_ms       Maximum wait time in milliseconds.
    /// @param out              Output object (filled on success).
    /// @param poll_ms          Per-iteration spin_once timeout in ms
    ///                         (default 10). Smaller values give lower
    ///                         latency when the reply arrives mid-step
    ///                         but wake the executor more often; larger
    ///                         values reduce wakeups for long waits.
    /// @return Result::success(), ErrorCode::Timeout, or error.
    Result wait(void* executor_handle, uint32_t timeout_ms, T& out, uint32_t poll_ms = 10) {
        if (slot_ < 0) return Result(ErrorCode::Error);
        if (poll_ms == 0) poll_ms = 1;
        // Phase 89.2: budget by wall-clock. Accumulating `step` per iteration
        // breaks when the underlying `zpico_spin_once` returns early on a
        // signaled condvar (keep-alives, discovery gossip) — the 500-step
        // default-timeout loop can then collapse into milliseconds and
        // return ErrorCode::Timeout before the reply has a chance to land.
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(timeout_ms) * 1000000ULL;
        while (true) {
            nros_cpp_ret_t ret = nros_cpp_spin_once(executor_handle, static_cast<int32_t>(poll_ms));
            // Transient conditions: keep polling. Anything else propagates.
            // - Ok (0): nothing to dispatch this round.
            // - Timeout (-2): spin_once returned after its timeout — normal.
            // - TryAgain (-6): transport hint to retry.
            if (ret != 0 && ret != static_cast<nros_cpp_ret_t>(ErrorCode::Timeout) &&
                ret != static_cast<nros_cpp_ret_t>(ErrorCode::TryAgain)) {
                return Result(ret);
            }
            if (is_ready()) return try_take(out);
            const uint64_t now_ns = nros_cpp_time_ns();
            if (now_ns - start_ns >= budget_ns) break;
        }
        return Result(ErrorCode::Timeout);
    }

    /// Cancel the pending operation (idempotent).
    void cancel() { slot_ = -1; }

    /// `std::future::valid()` — true until the result is taken or the future
    /// is cancelled (issue 1996). The negation of @ref is_consumed.
    bool valid() const { return slot_ >= 0; }

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
    /// `FutureAndRequestId::get()` — issue 1996. ADOPT-BOUNDED.
    ///
    /// Upstream returns the reply as an owning `std::shared_ptr<Response>`, and
    /// so does this: one heap copy of the reply, deserialized into a
    /// `std::make_shared<T>()`, which the caller may keep. It consumes the
    /// future, as `std::future::get()` does.
    ///
    /// THE BOUND: upstream BLOCKS until the reply arrives; this never waits.
    /// A wait that does not drive the executor is what RFC-0021 forbids, and
    /// a future has no executor to drive on its own. So call it where the
    /// stock examples do — after `rclcpp::spin_until_future_complete(node,
    /// fut)` returned `SUCCESS`, or `wait_for(...)` returned `ready`. Called
    /// before the reply (or on a consumed future) it says
    /// `NROS_RCLCPP_REFUSE_FUTURE_GET_NOT_READY_RUNTIME` and returns nullptr;
    /// a reply that fails to deserialize also returns nullptr.
    ///
    /// Absent on a freestanding build, where there is no `<memory>`: there the
    /// name is a `static_assert` naming `try_take` (`NROS_RCLCPP_REFUSE_FUTURE_GET`).
    ::std::shared_ptr<T> get() {
        if (slot_ < 0 || (!ready_ && !is_ready())) {
            NROS_RCLCPP_SAY_REFUSED(NROS_RCLCPP_REFUSE_FUTURE_GET_NOT_READY_RUNTIME);
            return ::std::shared_ptr<T>();
        }
        ::std::shared_ptr<T> out = ::std::make_shared<T>();
        if (!try_take(*out).ok()) return ::std::shared_ptr<T>();
        return out;
    }
#else
    /// Freestanding: `get()` is REFUSED at compile time — no `<memory>`, no
    /// allocator. The message names `try_take` and `wait` (issue 1996).
    template <typename U = void> void get() {
        static_assert(::rclcpp::detail::refuse<U>::value, NROS_RCLCPP_REFUSE_FUTURE_GET);
    }
#endif // NROS_CPP_HAS_SHARED_PTR

#if defined(NROS_CPP_HAS_STD_FUTURE) // hosted-family: future-interop
    /// `FutureAndRequestId::wait_for(duration)` — issue 1996. ADOPT-BOUNDED.
    ///
    /// Returns `std::future_status::ready` once the reply has arrived and
    /// `std::future_status::timeout` when the budget runs out first. It DRIVES
    /// the executor the client was created on while it waits, in `spin_once`
    /// slices of at most 10 ms — the wait RFC-0021 allows — so other entities
    /// keep being served and the reply can land on a single-threaded
    /// transport. Upstream's `wait_for` merely blocks on a promise a second
    /// thread fulfils.
    ///
    /// The budget is rounded UP to milliseconds and clamped to `UINT32_MAX`;
    /// a zero or negative duration is one non-waiting check, as for
    /// `std::future::wait_for`. Never `deferred`. On a consumed or cancelled
    /// future (where upstream's is undefined) it returns `timeout` at once.
    /// A future with no executor (default-constructed) does not wait: it checks once.
    template <typename Rep, typename Period>
    ::std::future_status wait_for(::std::chrono::duration<Rep, Period> timeout) {
        if (slot_ < 0) return ::std::future_status::timeout;
        if (ready_ || is_ready()) return ::std::future_status::ready;
        const int64_t ms = ::rclcpp::detail::chrono_wait_budget_ms(timeout);
        if (ms <= 0 || executor_ == nullptr) {
            if (executor_ != nullptr) (void)nros_cpp_spin_once(executor_, 0);
            return is_ready() ? ::std::future_status::ready : ::std::future_status::timeout;
        }
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(ms) * 1000000ULL;
        while (true) {
            const uint64_t elapsed_ns = nros_cpp_time_ns() - start_ns;
            if (elapsed_ns >= budget_ns) break;
            const uint64_t left_ms = (budget_ns - elapsed_ns + 999999ULL) / 1000000ULL;
            const int32_t slice = left_ms < 10ULL ? static_cast<int32_t>(left_ms) : 10;
            (void)nros_cpp_spin_once(executor_, slice);
            if (is_ready()) return ::std::future_status::ready;
        }
        return is_ready() ? ::std::future_status::ready : ::std::future_status::timeout;
    }
#endif // NROS_CPP_HAS_STD_FUTURE

    /// Check if the future has been consumed or cancelled.
    bool is_consumed() const { return slot_ < 0; }

    // Move semantics (non-copyable, single-shot)
    Future(Future&& other) noexcept
        : client_storage_(other.client_storage_), take_fn_(other.take_fn_),
          executor_(other.executor_), slot_(other.slot_), ready_(other.ready_),
          cached_len_(other.cached_len_) {
        for (size_t i = 0; i < cached_len_; ++i)
            cached_buf_[i] = other.cached_buf_[i];
        other.slot_ = -1;
        other.ready_ = false;
    }

    Future& operator=(Future&& other) noexcept {
        if (this != &other) {
            client_storage_ = other.client_storage_;
            take_fn_ = other.take_fn_;
            executor_ = other.executor_;
            slot_ = other.slot_;
            ready_ = other.ready_;
            cached_len_ = other.cached_len_;
            for (size_t i = 0; i < cached_len_; ++i)
                cached_buf_[i] = other.cached_buf_[i];
            other.slot_ = -1;
            other.ready_ = false;
        }
        return *this;
    }

    ~Future() { cancel(); }

    /// Default constructor -- creates an empty/consumed future.
    Future()
        : client_storage_(nullptr), take_fn_(nullptr), executor_(nullptr), slot_(-1), ready_(false),
          cached_len_(0) {}

  private:
    Future(const Future&) = delete;
    Future& operator=(const Future&) = delete;

    template <typename S> friend class ::rclcpp::PollClient;
    template <typename A> friend class ::rclcpp_action::Client;

    using TryRecvFn = nros_cpp_ret_t (*)(void*, uint8_t*, size_t, size_t*);

    // `executor` is what `wait_for` drives (issue 1996); nullptr = no
    // executor known, and `wait_for` then checks once instead of waiting.
    Future(void* storage, TryRecvFn fn, int slot, void* executor = nullptr)
        : client_storage_(storage), take_fn_(fn), executor_(executor), slot_(slot), ready_(false),
          cached_len_(0) {}

    void* client_storage_;
    TryRecvFn take_fn_;
    void* executor_;
    int slot_;
    bool ready_;
    size_t cached_len_;
    uint8_t cached_buf_[Cap];
};

} // namespace rclcpp
#endif // NROS_RCLCPP_FUTURE_HPP
