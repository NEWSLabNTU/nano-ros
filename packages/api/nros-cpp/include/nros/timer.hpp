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
#include "nros/hosted_block.hpp"
// phase-417 G6 — `time_until_trigger()` returns `nros::Duration`, so this
// header NAMES it and must be includable first (the rule `qos.hpp` states for
// the same dependency). `duration.hpp` includes nothing of ours.
#include "nros/duration.hpp"

// phase-476 W2 — nothing in this header is hosted-only any more: the pointer
// aliases name `TimerHandle` rather than `std::shared_ptr`, and the callable
// lives in the executor arena rather than a `std::function` cell. So it
// includes no capability probe.

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

class Timer;

/// What `timer_->...` reaches on a [`TimerHandle`]: the operations a timer
/// registration supports, over the same two words — phase-476 W2.
///
/// A separate type from the handle because ported code says BOTH
/// `timer_->reset()` (restart the timer) and `timer_.reset()` (drop it), and
/// upstream gives them different meanings. Behind one class, `operator->`
/// returning `this` would make the two spellings the same call.
class TimerOps {
  public:
    /// Stop the timer firing. It stays registered; `reset()` restarts it.
    Result cancel() { return Result(nros_cpp_timer_cancel(executor_, handle_id_)); }

    /// Restart the timer from zero elapsed time; un-cancels a cancelled one.
    Result reset() { return Result(nros_cpp_timer_reset(executor_, handle_id_)); }

    /// Whether the timer is cancelled. A released or empty handle answers true:
    /// nothing will fire it.
    bool is_canceled() const {
        return executor_ == nullptr || nros_cpp_timer_is_canceled(executor_, handle_id_);
    }

    /// Would the timer fire on the next `spin_once()`? — `TimerBase::is_ready()`.
    /// See `nros::Timer::is_ready`.
    bool is_ready() const {
        return executor_ != nullptr && nros_cpp_timer_is_ready(executor_, handle_id_);
    }

    /// Time until the timer next fires, NEGATIVE when overdue —
    /// `TimerBase::time_until_trigger()`. See `nros::Timer::time_until_trigger`
    /// for the two recorded differences from rclcpp. `Duration()` for an empty,
    /// released or stale handle.
    Duration time_until_trigger() const {
        int64_t ns = 0;
        if (executor_ == nullptr ||
            nros_cpp_timer_time_until_next_call_ns(executor_, handle_id_, &ns) != NROS_CPP_RET_OK) {
            return Duration();
        }
        return Duration::from_nanoseconds(ns);
    }

  protected:
    constexpr TimerOps(void* executor, size_t handle_id)
        : executor_(executor), handle_id_(handle_id) {}

    void* executor_;
    size_t handle_id_;
};

/// The handle `create_wall_timer` / `rclcpp::create_timer` return, and what
/// `Timer::SharedPtr` names — phase-476 W2.
///
/// Two words, trivially copyable, no allocation, and present on every target:
/// the timer's callable lives in the executor arena (phase-476 W2), so there is
/// nothing on the C++ side for a `std::shared_ptr` to own. That is what freed
/// `timer.hpp` from `<memory>` and `<functional>`.
///
/// `timer_->cancel()`, `timer_->reset()`, `timer_->is_ready()` and
/// `timer_->time_until_trigger()` keep their rclcpp spelling through
/// `operator->`, which reaches a [`TimerOps`].
///
/// WHAT DIFFERS FROM A `shared_ptr`, stated because a ported file cannot see it:
///
///  * The handle does not OWN the timer. Dropping or overwriting it leaves the
///    timer firing, where upstream destroys a timer when its last `shared_ptr`
///    goes. The node owns its timers: destroying the node releases them
///    (phase-476 W0). That matches what this API did before W2, when the node
///    co-owned a heap cell for every timer.
///  * `timer_.reset()` RELEASES the timer, for every copy at once: the closest
///    thing to upstream's "drop my reference" that a non-counting handle can
///    offer, and the spelling ported code uses to stop a timer for good.
///  * A copy that outlives a release is SAFE. The handle carries the generation
///    of the slot it was issued for (phase-476 W0), so every operation through a
///    stale copy fails instead of reaching whatever registration took the slot.
class TimerHandle : private TimerOps {
  public:
    /// What this handle refers to, for generic code that spells a pointee type
    /// the way `std::shared_ptr` exposes one. A NAME only: `operator->` reaches
    /// a [`TimerOps`], not a `Timer` object.
    using element_type = ::nros::Timer;

    constexpr TimerHandle() : TimerOps(nullptr, 0) {}
    /// Null, spelled the way a ported file spells it
    /// (`Timer::SharedPtr timer_ = nullptr;`).
    constexpr TimerHandle(decltype(nullptr)) : TimerOps(nullptr, 0) {}
    constexpr TimerHandle(void* executor, size_t handle_id) : TimerOps(executor, handle_id) {}

    /// `if (timer_)`. A default-constructed or `reset()` handle is empty; one
    /// from a successful registration is not (even once released elsewhere —
    /// the operations are what report that).
    explicit constexpr operator bool() const { return executor_ != nullptr; }

    /// `timer_->cancel()` and friends.
    TimerOps* operator->() { return this; }
    /// Const overload of `operator->`.
    const TimerOps* operator->() const { return this; }

    /// `timer_.reset()` — release the timer: it stops for good, its arena entry
    /// and captured callable are freed, and this handle becomes empty. Other
    /// copies become stale and fail safely. A no-op on an empty handle.
    void reset() {
        if (executor_ != nullptr) {
            (void)nros_cpp_timer_release(executor_, handle_id_);
        }
        executor_ = nullptr;
        handle_id_ = 0;
    }

    /// The packed executor handle this refers to. For introspection and tests.
    constexpr size_t handle_id() const { return handle_id_; }

    friend constexpr bool operator==(const TimerHandle& a, const TimerHandle& b) {
        return a.executor_ == b.executor_ && a.handle_id_ == b.handle_id_;
    }
    friend constexpr bool operator!=(const TimerHandle& a, const TimerHandle& b) {
        return !(a == b);
    }
    friend constexpr bool operator==(const TimerHandle& a, decltype(nullptr)) {
        return a.executor_ == nullptr;
    }
    friend constexpr bool operator!=(const TimerHandle& a, decltype(nullptr)) {
        return a.executor_ != nullptr;
    }
    friend constexpr bool operator==(decltype(nullptr), const TimerHandle& a) {
        return a.executor_ == nullptr;
    }
    friend constexpr bool operator!=(decltype(nullptr), const TimerHandle& a) {
        return a.executor_ != nullptr;
    }
};

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
    /// `Timer::SharedPtr` — how a timer member is declared:
    /// `rclcpp::Timer::SharedPtr timer_;` (or `rclcpp::TimerBase::SharedPtr`,
    /// the alias below).
    ///
    /// phase-476 W2 — a [`TimerHandle`], not a `std::shared_ptr<Timer>`, and
    /// UNCONDITIONAL: the timer's callable lives in the executor arena, so the
    /// C++ side owns nothing a smart pointer could hold, and a freestanding
    /// target gets the ported spelling too. See `TimerHandle` for what differs
    /// from a `shared_ptr` (it does not own the timer; `.reset()` releases it).
    using SharedPtr = ::nros::TimerHandle;
    /// `Timer::ConstSharedPtr` — the same handle; see `SharedPtr`.
    using ConstSharedPtr = ::nros::TimerHandle;
    /// `Timer::UniquePtr` — the same handle; see `SharedPtr`. Upstream's is a
    /// sole owner. This one owns nothing, like the other two spellings.
    using UniquePtr = ::nros::TimerHandle;

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

    /// Would this timer fire on the next `spin_once()` pass? — rclcpp's
    /// `TimerBase::is_ready()`.
    ///
    /// phase-417 G6. The answer is the EXECUTOR's, read from the arena entry
    /// the timer is registered in: a cancelled timer is never ready, a fired
    /// one-shot is never ready again, and otherwise `elapsed >= period`. That
    /// is `arena::timer_try_process`'s own guard rather than a second opinion
    /// about it — a readiness answer that can disagree with the dispatcher is
    /// worse than no answer.
    ///
    /// An uninitialized timer answers `false`: it is registered with no
    /// executor, so nobody will dispatch it.
    bool is_ready() const {
        if (!initialized_) return false;
        return nros_cpp_timer_is_ready(executor_, handle_id_);
    }

    /// Time until this timer next fires — NEGATIVE when it is overdue.
    /// rclcpp's `TimerBase::time_until_trigger()`.
    ///
    /// phase-417 G6. Two deliberate differences from rclcpp, both recorded on
    /// the ledger row:
    ///
    ///  * upstream returns `std::chrono::nanoseconds`; this returns
    ///    [`nros::Duration`], because this header is freestanding and
    ///    `<chrono>` is not reachable from every target it serves (issue 0112).
    ///    The UNIT is the same — `d.nanoseconds()` is upstream's count.
    ///  * the arena's timer accounting is MICROSECOND-based (issue #505), so
    ///    the nanosecond value is a microsecond quantity scaled by 1000. The
    ///    unit is rclcpp's; the resolution is ours.
    ///
    /// A wall timer answers on the platform steady clock; a timer created with
    /// `create_timer(clock, …)` answers on its own clock.
    ///
    /// `Duration()` (zero) for an uninitialized timer or one the executor does
    /// not know. That collides with "fires exactly now", which is why
    /// [`is_valid`] is the question to ask first — the C surface spends a
    /// return code on the distinction (`rcl_timer_get_time_until_next_call`
    /// answers `NROS_RET_NOT_INIT`) and a `Duration`-returning accessor has no
    /// room for one.
    Duration time_until_trigger() const {
        if (!initialized_) return Duration();
        int64_t ns = 0;
        if (nros_cpp_timer_time_until_next_call_ns(executor_, handle_id_, &ns) != NROS_CPP_RET_OK) {
            return Duration();
        }
        return Duration::from_nanoseconds(ns);
    }

    /// Check if the timer is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — cancels the timer.
    ~Timer() {
        if (initialized_) {
            nros_cpp_timer_cancel(executor_, handle_id_);
            initialized_ = false;
        }
        // The closure block (if any) is freed here; the runtime no longer
        // holds a raw pointer into it because we cancelled above.
        detail::destroy_hosted_block(closure_);
    }

    // Move semantics (non-copyable)
    Timer(Timer&& other)
        : executor_(other.executor_), handle_id_(other.handle_id_),
          initialized_(other.initialized_), closure_(other.closure_) {
        other.executor_ = nullptr;
        other.initialized_ = false;
        other.closure_ = nullptr;
    }

    Timer& operator=(Timer&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_timer_cancel(executor_, handle_id_);
            }
            executor_ = other.executor_;
            handle_id_ = other.handle_id_;
            initialized_ = other.initialized_;
            detail::destroy_hosted_block(closure_);
            closure_ = other.closure_;
            other.executor_ = nullptr;
            other.initialized_ = false;
            other.closure_ = nullptr;
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized timer.
    /// Use `Node::create_wall_timer()` to initialize.
    Timer() : executor_(nullptr), handle_id_(0), initialized_(false), closure_(nullptr) {}

    /// @internal Take ownership of a closure block for this timer.
    ///
    /// `block` must be the address of the `detail::HostedBlockBase` subobject
    /// of a block allocated by the caller, or null. It is called *after* the
    /// runtime has registered a raw callback pointing into that same block. The
    /// Timer frees it on destruction, so the raw pointer the runtime holds is
    /// never dereferenced after free — the destructor cancels first.
    ///
    /// Unconditional, and takes a `void*`, because the MEMBER is
    /// unconditional: see `detail::HostedBlockBase`. Not intended for user
    /// code.
    void attach_closure_block(detail::HostedBlockBase* block) {
        detail::destroy_hosted_block(closure_);
        closure_ = block;
    }

  private:
    Timer(const Timer&) = delete;
    Timer& operator=(const Timer&) = delete;

    friend class ::rclcpp::Node;

    void* executor_;
    size_t handle_id_;
    bool initialized_;

    /// Owns the closure block (if any) — `detail::HostedBlockBase*`, erased.
    ///
    /// Only populated when the Timer was created through a convenience
    /// wrapper that had a closure to keep alive; a timer created with a plain
    /// C callback leaves it null. Freed automatically when the Timer is
    /// destroyed or moved-from.
    ///
    /// UNCONDITIONAL, and that is the point (issue 1225, phase-442 W1): a
    /// member behind `NROS_CPP_STD` made `sizeof(nros::Timer)` 24 or 32
    /// depending on a flag one module of an image may set on its own, and
    /// carried `nros::ComponentNode` (`Timer timers_[8]`) and the
    /// `rclcpp::Timer` / `rclcpp::TimerBase` aliases with it.
    void* closure_;
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
//   3. THE FLAT SHAPE IS A STRICTLY BETTER KEEP-ALIVE. `create_wall_timer`
//      returned `std::shared_ptr<::nros::Timer>` aliased onto a private cell,
//      as `create_subscription` did. Since phase-476 W2 both return a two-word
//      handle and there is no cell: the callable is in the executor arena.
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

/// `rclcpp::TimerBase` — the PORTED NAME for the one flat timer type. It is an
/// ALIAS for `rclcpp::Timer`, not a base class: `std::is_same<TimerBase,
/// Timer>::value` is true and no vtable exists behind either spelling.
///
/// KEPT, and NOT deprecated, and the reason is MEASURED rather than stylistic.
/// phase-430 W7 first deleted the name outright — RFC-0089 §"Timer, studied
/// against RTOS semantics" argues that an alias "sells a taxonomy we do not
/// have". The `colcon-parity` gate then failed, and it named the constraint
/// that argument had not considered:
///
///     rclcpp::Timer  ->  'Timer' in namespace 'rclcpp' does not name a type;
///                        did you mean 'Time'?      [REAL ROS 2, humble]
///
/// `examples/templates/local-msg-package` is built by `just colcon-parity`
/// against a real `/opt/ros/<distro>` install, and
/// `examples/templates/cpp-port-minimal-publisher` is vendored UNMODIFIED to
/// demonstrate that upstream source compiles here. Both declare a timer member.
/// Upstream has no `rclcpp::Timer`, so WITHOUT this alias the intersection of
/// "compiles under real ROS 2" and "compiles under nano-ros" is EMPTY for any
/// ported node that holds a timer — which is most of them. The alias is what
/// keeps that intersection non-empty; it is not a courtesy, and deprecating it
/// would promise a removal that would break the property on purpose.
///
/// WHAT WAS ACTUALLY DELETED, and it is where the cost was: the HIERARCHY.
/// `class TimerBase` had a virtual destructor and `detail::WallTimer` derived
/// from it. The executor dispatches through a raw `void(*)(void*)` recovered by
/// a STATIC trampoline, so no virtual call through a `TimerBase*` existed or
/// can — a vtable with no caller, which clause 1 refuses. phase-476 W2 then
/// deleted `detail::WallTimer` itself; `create_wall_timer` returns a
/// `TimerHandle`.
///
/// WHAT THE NAME STILL DOES NOT PROMISE: `WallTimer` and `GenericTimer` stay
/// absent, so `rclcpp::WallTimer<...>` does not compile; and a file that
/// DERIVES from `TimerBase` is deriving from a concrete, non-polymorphic handle
/// whose destructor is not virtual. Both are documented divergences with ledger
/// rows rather than silent ones.
using TimerBase = ::nros::Timer;

// phase-476 W2 deleted `detail::WallTimer`, the heap cell (a `std::function`
// plus an `nros::Timer`) that gave a capturing timer callback a stable address
// and that the node kept alive through `owned_entities`. The callable now lives
// in the executor arena (`nros_cpp_timer_create_capturing`), destroyed when the
// timer is released, and the returned handle is two words.

} // namespace rclcpp

#endif // NROS_CPP_TIMER_HPP
