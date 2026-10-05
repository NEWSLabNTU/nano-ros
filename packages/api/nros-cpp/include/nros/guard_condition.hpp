// nros-cpp: Guard condition class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file guard_condition.hpp
 * @ingroup grp_executor
 * @brief `nros::GuardCondition` — cross-thread wake source.
 */

#ifndef NROS_CPP_GUARD_CONDITION_HPP
#define NROS_CPP_GUARD_CONDITION_HPP

#include <cstdint>
#include <cstddef>

#include "nros/config.hpp"
#include "nros/result.hpp"
#include "nros/hosted_block.hpp"

#include "nros_cpp_ffi.h"

// phase-427 W7 — `Node` is DEFINED in `rclcpp::` (RFC-0089: that namespace is
// the home). The friend declaration below is qualified, and a qualified friend
// names an existing entity rather than introducing one, so the name has to be
// declared first — and in `rclcpp::`, because an elaborated `class Node;` in
// `nros::` would declare a second, distinct class.
namespace rclcpp {
class Node;
}

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

    /// Is this guard condition triggered and not yet dispatched?
    ///
    /// The RFC-0022 polling tier: a task that owns its own loop and never
    /// reaches `spin_once` still needs to see the flag another thread or an ISR
    /// set. rcl and rclcpp have no such reader — a wait set is the only way to
    /// observe an rclcpp guard condition — so this is ours, and phase-417 W4.e
    /// gave C++ and Rust the reader C already had.
    ///
    /// The executor CONSUMES the flag when it dispatches, so this answers "set
    /// and not yet dispatched": poll it or take the callback, not both.
    bool is_triggered() const {
        if (!initialized_) return false;
        return nros_cpp_guard_condition_is_triggered(storage_);
    }

    /// Clear the flag without dispatching — the other half of `is_triggered()`,
    /// for a polling owner that has handled the event itself.
    Result clear() {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_guard_condition_clear(storage_));
    }

    /// Destructor — releases the guard condition (issue 1667): its executor
    /// entry is freed, so `spin_once` stops polling it and the callback slot
    /// goes back to `NROS_EXECUTOR_MAX_CBS`. Sound because this object is
    /// non-copyable and the only holder of the trigger handle; trigger it from
    /// another thread only while it is alive.
    ~GuardCondition() {
        release();
        // The closure block (if any) is freed here, after the entry that could
        // call into it.
        detail::destroy_hosted_block(closure_);
    }

    // Move semantics (non-copyable). Relocation goes through the
    // `nros_cpp_guard_condition_relocate` runtime call (Phase 84.C1).
    GuardCondition(GuardCondition&& other)
        : initialized_(other.initialized_), closure_(other.closure_), executor_(other.executor_),
          handle_id_(other.handle_id_) {
        if (other.initialized_) {
            nros_cpp_guard_condition_relocate(other.storage_, storage_);
            other.initialized_ = false;
        }
        other.closure_ = nullptr;
        other.executor_ = nullptr;
        other.handle_id_ = 0;
    }

    GuardCondition& operator=(GuardCondition&& other) {
        if (this != &other) {
            release();
            if (other.initialized_) {
                nros_cpp_guard_condition_relocate(other.storage_, storage_);
                initialized_ = true;
                other.initialized_ = false;
            }
            executor_ = other.executor_;
            handle_id_ = other.handle_id_;
            other.executor_ = nullptr;
            other.handle_id_ = 0;
            detail::destroy_hosted_block(closure_);
            closure_ = other.closure_;
            other.closure_ = nullptr;
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized guard condition.
    /// Use `Node::create_guard_condition()` to initialize.
    GuardCondition()
        : storage_(), initialized_(false), closure_(nullptr), executor_(nullptr), handle_id_(0) {}

    /// @internal Take ownership of a closure block. See
    /// `Timer::attach_closure_block` for rationale. Not intended for user
    /// code — called by the `NROS_CPP_STD` convenience wrappers.
    void attach_closure_block(detail::HostedBlockBase* block) {
        detail::destroy_hosted_block(closure_);
        closure_ = block;
    }

  private:
    /// Free the executor entry, then drop the local handle. Idempotent.
    void release() {
        if (initialized_) {
            if (executor_ != nullptr) {
                (void)nros_cpp_guard_condition_release(executor_, handle_id_);
            }
            nros_cpp_guard_condition_destroy(storage_);
            initialized_ = false;
        }
        executor_ = nullptr;
        handle_id_ = 0;
    }

    GuardCondition(const GuardCondition&) = delete;
    GuardCondition& operator=(const GuardCondition&) = delete;

    friend class ::rclcpp::Node;

    alignas(8) uint8_t storage_[NROS_GUARD_CONDITION_SIZE];
    bool initialized_;

    /// Owns the closure block (if any) — `detail::HostedBlockBase*`, erased.
    /// UNCONDITIONAL: see `Timer::closure_` and issue 1225.
    void* closure_;

    /// The executor and the generation-carrying entry handle, for the release.
    void* executor_;
    size_t handle_id_;
};

} // namespace nros

#endif // NROS_CPP_GUARD_CONDITION_HPP
