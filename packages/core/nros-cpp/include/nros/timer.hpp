// nros-cpp: Timer class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file timer.hpp
 * @ingroup grp_executor
 * @brief `nros::Timer` — periodic callback driven by the executor.
 */

#ifndef NROS_CPP_TIMER_HPP
#define NROS_CPP_TIMER_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

#ifdef NROS_CPP_STD
#include <functional>
#include <memory>
#endif

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
typedef void (*nros_cpp_timer_callback_t)(void* context);
nros_cpp_ret_t nros_cpp_timer_create(void* executor_handle, uint64_t period_ms,
                                     nros_cpp_timer_callback_t callback, void* context,
                                     size_t* out_handle_id);
nros_cpp_ret_t nros_cpp_timer_create_oneshot(void* executor_handle, uint64_t delay_ms,
                                             nros_cpp_timer_callback_t callback, void* context,
                                             size_t* out_handle_id);
nros_cpp_ret_t nros_cpp_timer_cancel(void* executor_handle, size_t handle_id);
nros_cpp_ret_t nros_cpp_timer_reset(void* executor_handle, size_t handle_id);
bool nros_cpp_timer_is_cancelled(void* executor_handle, size_t handle_id);
} // extern "C"

namespace nros {

/// Repeating or one-shot timer registered with the executor.
///
/// Timers fire during `spin_once()` when their period has elapsed.
/// The callback is a C function pointer with a user context.
///
/// Usage:
/// ```cpp
/// void on_timer(void* ctx) { /* periodic work */ }
///
/// nros::Timer timer;
/// NROS_TRY(node.create_timer(timer, 1000, on_timer));  // 1000ms period
/// // timer fires during nros::spin_once()
/// timer.cancel();
/// timer.reset();  // restart from zero
/// ```
class Timer {
  public:
    /// Cancel the timer. It stops firing but remains in the executor.
    /// Use `reset()` to restart it.
    Result cancel() {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_timer_cancel(executor_, handle_id_));
    }

    /// Reset the timer (restart from zero elapsed time).
    /// If cancelled, this also un-cancels it.
    Result reset() {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_timer_reset(executor_, handle_id_));
    }

    /// Check if the timer is cancelled.
    bool is_cancelled() const {
        if (!initialized_) return true;
        return nros_cpp_timer_is_cancelled(executor_, handle_id_);
    }

    /// Check if the timer is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — cancels the timer.
    ~Timer() {
        if (initialized_) {
            nros_cpp_timer_cancel(executor_, handle_id_);
            initialized_ = false;
        }
        // closure_ (if any) destructs here; the runtime no longer
        // holds a raw pointer to it because we cancelled above.
    }

    // Move semantics (non-copyable)
    Timer(Timer&& other)
        : executor_(other.executor_), handle_id_(other.handle_id_), initialized_(other.initialized_)
#ifdef NROS_CPP_STD
          ,
          closure_(std::move(other.closure_))
#endif
    {
        other.executor_ = nullptr;
        other.initialized_ = false;
    }

    Timer& operator=(Timer&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_timer_cancel(executor_, handle_id_);
            }
            executor_ = other.executor_;
            handle_id_ = other.handle_id_;
            initialized_ = other.initialized_;
#ifdef NROS_CPP_STD
            closure_ = std::move(other.closure_);
#endif
            other.executor_ = nullptr;
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized timer.
    /// Use `Node::create_timer()` to initialize.
    Timer() : executor_(nullptr), handle_id_(0), initialized_(false) {}

#ifdef NROS_CPP_STD
    /// @internal Attach a heap-allocated std::function closure to this
    /// timer. Called by the `NROS_CPP_STD` convenience wrappers in
    /// `std_compat.hpp` *after* the runtime registered a raw callback
    /// pointing into the same closure. The unique_ptr keeps the closure
    /// alive for the lifetime of the Timer, freeing it automatically on
    /// destruction. Not intended for user code.
    void attach_std_closure(std::unique_ptr<std::function<void()>> closure) {
        closure_ = std::move(closure);
    }
#endif

  private:
    Timer(const Timer&) = delete;
    Timer& operator=(const Timer&) = delete;

    friend class Node;

    void* executor_;
    size_t handle_id_;
    bool initialized_;

#ifdef NROS_CPP_STD
    /// Owns the heap-allocated `std::function<void()>` closure (if any).
    ///
    /// Only populated when the Timer was created through the
    /// `NROS_CPP_STD` convenience wrapper. Freed automatically when the
    /// Timer is destroyed or moved-from.
    std::unique_ptr<std::function<void()>> closure_;
#endif
};

} // namespace nros

#endif // NROS_CPP_TIMER_HPP
