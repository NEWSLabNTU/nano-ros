// nros-cpp: Guard condition class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_GUARD_CONDITION_HPP
#define NROS_CPP_GUARD_CONDITION_HPP

#include <cstdint>
#include <cstddef>

#include "nros/config.hpp"
#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
typedef void (*nros_cpp_guard_callback_t)(void* context);
nros_cpp_ret_t nros_cpp_guard_condition_create(void* executor_handle,
                                               nros_cpp_guard_callback_t callback, void* context,
                                               void* storage);
nros_cpp_ret_t nros_cpp_guard_condition_trigger(void* storage);
nros_cpp_ret_t nros_cpp_guard_condition_destroy(void* storage);
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
        return Result(nros_cpp_guard_condition_trigger(storage_));
    }

    /// Check if the guard condition is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases guard condition resources.
    ~GuardCondition() {
        if (initialized_) {
            nros_cpp_guard_condition_destroy(storage_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    GuardCondition(GuardCondition&& other) : initialized_(other.initialized_) {
        for (unsigned i = 0; i < sizeof(storage_); ++i) {
            storage_[i] = other.storage_[i];
            other.storage_[i] = 0;
        }
        other.initialized_ = false;
    }

    GuardCondition& operator=(GuardCondition&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_guard_condition_destroy(storage_);
            }
            for (unsigned i = 0; i < sizeof(storage_); ++i) {
                storage_[i] = other.storage_[i];
                other.storage_[i] = 0;
            }
            initialized_ = other.initialized_;
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized guard condition.
    /// Use `Node::create_guard_condition()` to initialize.
    GuardCondition() : storage_(), initialized_(false) {}

  private:
    GuardCondition(const GuardCondition&) = delete;
    GuardCondition& operator=(const GuardCondition&) = delete;

    friend class Node;

    alignas(8) uint8_t storage_[NROS_CPP_GUARD_CONDITION_STORAGE_SIZE];
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_GUARD_CONDITION_HPP
