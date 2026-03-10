// nros-cpp: Guard condition class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_GUARD_CONDITION_HPP
#define NROS_CPP_GUARD_CONDITION_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
typedef void (*nros_cpp_guard_callback_t)(void* context);
nros_cpp_ret_t nros_cpp_guard_condition_create(void* executor_handle,
                                               nros_cpp_guard_callback_t callback, void* context,
                                               void** out_handle);
nros_cpp_ret_t nros_cpp_guard_condition_trigger(void* handle);
nros_cpp_ret_t nros_cpp_guard_condition_destroy(void* handle);
} // extern "C"

namespace nros {

/// Guard condition for cross-thread signaling.
///
/// Guard conditions allow any thread to wake the executor and optionally
/// invoke a callback during `spin_once()`. The `trigger()` method is
/// thread-safe and lock-free.
///
/// Usage:
/// ```cpp
/// void on_signal(void* ctx) { /* handle event */ }
///
/// nros::GuardCondition guard;
/// NROS_TRY(node.create_guard_condition(guard, on_signal));
///
/// // From another thread:
/// guard.trigger();
/// // Callback fires on next spin_once()
/// ```
class GuardCondition {
  public:
    /// Trigger the guard condition (thread-safe, lock-free).
    ///
    /// The callback will be invoked on the next `spin_once()`.
    Result trigger() {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_guard_condition_trigger(handle_));
    }

    /// Check if the guard condition is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases guard condition resources.
    ~GuardCondition() {
        if (initialized_) {
            nros_cpp_guard_condition_destroy(handle_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    GuardCondition(GuardCondition&& other)
        : handle_(other.handle_), initialized_(other.initialized_) {
        other.handle_ = nullptr;
        other.initialized_ = false;
    }

    GuardCondition& operator=(GuardCondition&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_guard_condition_destroy(handle_);
            }
            handle_ = other.handle_;
            initialized_ = other.initialized_;
            other.handle_ = nullptr;
            other.initialized_ = false;
        }
        return *this;
    }

  private:
    GuardCondition(const GuardCondition&) = delete;
    GuardCondition& operator=(const GuardCondition&) = delete;

    friend class Node;
    GuardCondition() : handle_(nullptr), initialized_(false) {}

    void* handle_;
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_GUARD_CONDITION_HPP
