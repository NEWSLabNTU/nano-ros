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

// phase-417 W1.a — `<memory>` for the nested pointer aliases. Rationale (and
// why the test is `__has_include` rather than `__STDC_HOSTED__`, issue 0112)
// lives in `publisher.hpp`.
#if defined(NROS_CPP_STD)
#include <memory>
#define NROS_CPP_HAS_SHARED_PTR 1
#elif defined(__has_include)
#if __has_include(<memory>)
#include <memory>
#define NROS_CPP_HAS_SHARED_PTR 1
#endif
#endif

#include "nros_cpp_ffi.h"

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
/// NROS_TRY(node.create_wall_timer(timer, 1000, on_timer));  // 1000ms period
/// // timer fires during nros::spin_once()
/// timer.cancel();
/// timer.reset();  // restart from zero
/// ```
class Timer {
  public:
#ifdef NROS_CPP_HAS_SHARED_PTR
    /// `Timer::SharedPtr` — how a timer member is declared:
    /// `rclcpp::Timer::SharedPtr timer_;`.
    ///
    /// Upstream spells that `rclcpp::TimerBase::SharedPtr`. We do not have a
    /// `TimerBase`, deliberately — phase-430 W7 deleted the one-leaf hierarchy
    /// phase-417 W1.a had added, because the executor dispatches through a raw
    /// function pointer and a base class would be a vtable no dispatch uses.
    /// The rename is the mechanical edit the compile-or-conform rule wants.
    ///
    /// Ergonomics only (RFC-0089 §"Who implements an adopted name"): a
    /// spelling for `std::shared_ptr<Timer>`, no second code path.
    ///
    /// Present only where `<memory>` is — a freestanding target has no
    /// `std::shared_ptr` to alias.
    using SharedPtr = std::shared_ptr<Timer>;
    /// `Timer::ConstSharedPtr` — see `SharedPtr`.
    using ConstSharedPtr = std::shared_ptr<const Timer>;
    /// `Timer::UniquePtr` — see `SharedPtr`.
    using UniquePtr = std::unique_ptr<Timer>;
#endif

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
    bool is_canceled() const {
        if (!initialized_) return true;
        return nros_cpp_timer_is_canceled(executor_, handle_id_);
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
    /// Use `Node::create_wall_timer()` to initialize.
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

// ============================================================================
// rclcpp::Timer — FLAT. No `TimerBase`, no hierarchy (phase-430 W7 ruling)
// ============================================================================
//
// RFC-0089 §"Timer, studied against RTOS semantics" decided this and the tree
// disagreed with it: phase-417 W1.a had landed `class TimerBase` with a virtual
// destructor and `detail::WallTimer : TimerBase` under it — exactly the
// hierarchy the RFC refused. phase-430 W7 called for a ruling either way. The
// ruling is DELETE, and it is argued from the executor's dispatch:
//
//   1. THE VTABLE HAS NO CALLER. The only virtual member was `~TimerBase()`,
//      and there is no virtual call through a `TimerBase*` anywhere in the
//      tree — there cannot be. The executor's callback slot is
//      `nros_cpp_timer_callback_t`, a raw `void(*)(void*)`, and dispatch goes
//      through the STATIC `detail::WallTimer::trampoline`. The executor never
//      holds a C++ timer object, so it never needs a type-erased base. Even
//      the destructor's virtuality was dead: the cell is built with
//      `std::make_shared<detail::WallTimer>()`, so the control block already
//      records the CONCRETE deleter and destruction through a base pointer was
//      correct without it. A vtable no dispatch uses is cost with no caller,
//      which is what clause 1 of the governing principle refuses.
//
//   2. THE NAME PROMISES CHILDREN WE REFUSE TO HAVE. `WallTimer` and
//      `GenericTimer` are upstream's siblings under `TimerBase`, and both stay
//      absent by decision: the clock axis is a RUNTIME FIELD on the flat timer
//      plus a second VERB (`create_timer(clock, …)` beside `create_wall_timer`),
//      never a type parameter — phase-425 landed ROS time in exactly that
//      shape. A base whose one leaf is `detail::`-private advertises a taxonomy
//      the header itself refuses two paragraphs later.
//
//   3. THE FLAT SHAPE IS A STRICTLY BETTER KEEP-ALIVE. `create_wall_timer` now
//      returns `std::shared_ptr<::nros::Timer>` aliased onto the private cell,
//      which is what `create_subscription` has always done. The returned type
//      is the type that actually exists, and the cell stays an implementation
//      detail instead of being half-exposed as a base class.
//
// The migration cost is one mechanical rename the compiler demands:
// `rclcpp::TimerBase::SharedPtr timer_;` becomes `rclcpp::Timer::SharedPtr
// timer_;`. That is the compile-or-conform rule working as designed, and it is
// what the RFC calls the honest outcome — we do not have the hierarchy, so we
// do not take the name for it. Zero non-test call sites in this tree used it.
//
// `rclcpp::Timer` is an ours-only name in upstream's namespace (RFC-0089
// §"Settled: `nros::` is phased out entirely"), so it carries a ledger row with
// `disposition: extension` and the collision gate watches for `rclcpp::Timer`
// appearing in the recorded upstream surface.
//
// ADOPT-BOUNDED, and both halves of the envelope come with the executor:
//
//   * PERIOD RESOLUTION IS ONE MILLISECOND. `nros_cpp_timer_create` takes a
//     `uint64_t period_ms`, so `create_wall_timer(500us, …)` truncates to 0 and
//     fires every spin. Activations land on spin boundaries either way, so the
//     achievable cadence was already the spin period — the truncation only
//     makes the floor explicit.
//   * MISSED DEADLINES CATCH UP, where rcl's DROP. `TimerState::fire` keeps the
//     overshoot (`elapsed_ms -= period_ms`, `nros-node/src/timer.rs:298`), so
//     after a stall the callback runs once per `spin_once` until the backlog is
//     drained and the mean cadence is preserved. `rcl_timer_call` instead skips
//     whole missed periods and re-phases onto the grid, firing once. Closing
//     the gap means a missed-deadline POLICY on the executor's timer —
//     Rust-side work, not a loop re-added here (issue 1041).
//
// phase-417 — ONE DISPATCH PATH. `create_wall_timer` registers an EXECUTOR
// timer via `rclcpp::Node::create_wall_timer` (`node.hpp`), so the period
// arithmetic, the missed-deadline policy and the clock are the executor's,
// Rust-side. Until this landed, `WallTimer` carried its own
// `std::chrono::steady_clock` deadline and a `Node::pump()` fired it from
// `rclcpp::spin` / `spin_some` ONLY. Scheduling in the wrapper is
// RFC-0019/RFC-0020 violation class 2. Do not reintroduce a second dispatch
// loop.

// `<functional>` for the type-erased callback cell. Gated for the same reason
// `<memory>` is above — issue 0112, rationale in `publisher.hpp`.
#if defined(NROS_CPP_STD)
#include <functional>
#define NROS_CPP_HAS_STD_FUNCTION 1
#elif defined(__has_include)
#if __has_include(<functional>)
#include <functional>
#define NROS_CPP_HAS_STD_FUNCTION 1
#endif
#endif

namespace rclcpp {

/// `rclcpp::Timer` — the ROS 2 spelling of `nros::Timer`, and the whole timer
/// taxonomy this API has.
///
/// UNCONDITIONAL, unlike the `TimerBase` it replaces: `nros::Timer` needs no
/// `<memory>`, so a freestanding target gets the ROS 2 name too. The nested
/// `SharedPtr` / `ConstSharedPtr` / `UniquePtr` aliases live on `nros::Timer`
/// itself and are present where `<memory>` is, which is the only part that was
/// ever hosted-only.
using Timer = ::nros::Timer;

/// `rclcpp::TimerBase` — **RETIRED**, and an alias for one release so a ported
/// file gets the migration in the compiler's own words rather than
/// `'TimerBase' in namespace 'rclcpp' does not name a type`, which GCC offers
/// no suggestion for (measured).
///
/// THIS IS NOT THE PORTED-ALIAS PROPOSAL RFC-0089 REFUSED. That one made
/// `TimerBase` a first-class ported NAME, permanently, which sells a taxonomy
/// we do not have. This is the second step of the same document's own two-step
/// ("alias, then deprecate, then remove"), pointing the other way: the name is
/// GOING, and the diagnostic says so — including that there is no hierarchy
/// here, so a file that DERIVES from it is being told it is deriving from a
/// concrete handle.
///
/// The hierarchy itself IS deleted, which was phase-430 W7's actual ruling:
/// `detail::WallTimer` derives from nothing, `create_wall_timer` returns
/// `std::shared_ptr<Timer>`, and no vtable is emitted for any timer.
using TimerBase NROS_CPP_DEPRECATED_MSG(
    "rclcpp::TimerBase is retired: nano-ros has no timer hierarchy (the executor "
    "dispatches through a raw function pointer, so a polymorphic base would be a "
    "vtable nothing calls). Write rclcpp::Timer -- e.g. rclcpp::Timer::SharedPtr "
    "timer_;. WallTimer and GenericTimer are absent by design; the clock axis is "
    "rclcpp::create_timer(node, clock, period, cb), not a type parameter.") = ::nros::Timer;

#ifdef NROS_CPP_HAS_STD_FUNCTION
namespace detail {

/// An executor-registered timer plus the heap cell holding the user's callable.
///
/// TYPE ERASURE IS THE ONLY THING THIS ADDS. The executor's callback slot is
/// `nros_cpp_timer_callback_t` — `void(*)(void* ctx)` — and a ported rclcpp
/// timer callback is a capturing lambda or a `std::bind` result, which cannot
/// convert to a function pointer. `trampoline` recovers the cell from `ctx` and
/// calls it. That is a spelling, not a second code path (RFC-0089 §"Who
/// implements an adopted name"); no schedule, no clock read, no ordering.
///
/// NOT A CLASS HIERARCHY. It derived from `rclcpp::TimerBase` until phase-430
/// W7 deleted that base; it is a private implementation cell, and
/// `create_wall_timer` hands back a `std::shared_ptr<::nros::Timer>` aliased
/// onto its `timer` member rather than a pointer to the cell itself.
///
/// LIFETIME: the arena stores `this` as the dispatch context and nothing
/// unregisters it, so the cell has to outlive the registration.
/// `rclcpp::Node`'s `owned_entities_` holds a `shared_ptr` for the node's
/// lifetime, and the MEMBER ORDER below is load-bearing — members destruct in
/// reverse declaration order, so `timer` goes first and `~nros::Timer` cancels
/// the arena slot before `callback` is destroyed. Declared the other way round,
/// a tick landing between the two destructions would run a destroyed
/// `std::function`.
class WallTimer {
  public:
    static void trampoline(void* ctx) {
        auto* self = static_cast<WallTimer*>(ctx);
        if (self != nullptr && self->callback) {
            self->callback();
        }
    }

    std::function<void()> callback; // destroyed LAST
    ::nros::Timer timer;            // destroyed FIRST — cancels the arena slot
};

} // namespace detail
#endif // NROS_CPP_HAS_STD_FUNCTION

} // namespace rclcpp

#endif // NROS_CPP_TIMER_HPP
