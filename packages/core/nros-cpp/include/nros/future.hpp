// nros-cpp: Future<T> -- single-shot deferred result
// Freestanding C++ -- no exceptions, no STL required

#ifndef NROS_CPP_FUTURE_HPP
#define NROS_CPP_FUTURE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_spin_once(void* handle, int32_t timeout_ms);
/// Monotonic time in nanoseconds. Used by Future::wait() to budget its
/// spin loop by wall-clock, not iteration count (Phase 89.2).
uint64_t nros_cpp_time_ns(void);
}

namespace nros {

/// Single-shot deferred result for request/response operations.
///
/// Returned by `Client<S>::send_request()`. The future is consumed
/// when the result is taken -- move-only, single-shot.
///
/// Usage:
/// ```cpp
/// auto fut = client.send_request(req);
/// ResponseType resp;
/// NROS_TRY(fut.wait(executor.handle(), 5000, resp));
/// ```
template <typename T> class Future {
  public:
    /// Check if the result has arrived (non-blocking).
    bool is_ready() {
        if (slot_ < 0 || !try_recv_fn_) return false;
        if (ready_) return true;
        uint8_t buf[T::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        nros_cpp_ret_t ret = try_recv_fn_(client_storage_, buf, sizeof(buf), &len);
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
            nros_cpp_ret_t ret =
                nros_cpp_spin_once(executor_handle, static_cast<int32_t>(poll_ms));
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

    /// Check if the future has been consumed or cancelled.
    bool is_consumed() const { return slot_ < 0; }

    // Move semantics (non-copyable, single-shot)
    Future(Future&& other) noexcept
        : client_storage_(other.client_storage_), try_recv_fn_(other.try_recv_fn_),
          slot_(other.slot_), ready_(other.ready_), cached_len_(other.cached_len_) {
        for (size_t i = 0; i < cached_len_; ++i)
            cached_buf_[i] = other.cached_buf_[i];
        other.slot_ = -1;
        other.ready_ = false;
    }

    Future& operator=(Future&& other) noexcept {
        if (this != &other) {
            client_storage_ = other.client_storage_;
            try_recv_fn_ = other.try_recv_fn_;
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
        : client_storage_(nullptr), try_recv_fn_(nullptr), slot_(-1), ready_(false),
          cached_len_(0) {}

  private:
    Future(const Future&) = delete;
    Future& operator=(const Future&) = delete;

    template <typename S> friend class Client;
    template <typename A> friend class ActionClient;

    using TryRecvFn = nros_cpp_ret_t (*)(void*, uint8_t*, size_t, size_t*);

    Future(void* storage, TryRecvFn fn, int slot)
        : client_storage_(storage), try_recv_fn_(fn), slot_(slot), ready_(false), cached_len_(0) {}

    void* client_storage_;
    TryRecvFn try_recv_fn_;
    int slot_;
    bool ready_;
    size_t cached_len_;
    uint8_t cached_buf_[T::SERIALIZED_SIZE_MAX];
};

} // namespace nros
#endif // NROS_CPP_FUTURE_HPP
