// nros-cpp: Optional std mode conveniences
// Requires hosted C++ with STL — guarded by NROS_CPP_STD
//
// Provides:
// A) std::function<void()> callback wrappers for Timer and GuardCondition
// B) std::string forwarding overloads for name parameters
// C) std::chrono::milliseconds overloads for spin/timer durations

/**
 * @file std_compat.hpp
 * @ingroup grp_support
 * @brief `NROS_CPP_STD` opt-in conveniences — `std::function`,
 *        `std::string`, `std::chrono` overloads.
 */

#ifndef NROS_CPP_STD_COMPAT_HPP
#define NROS_CPP_STD_COMPAT_HPP

// This header is a no-op without NROS_CPP_STD (allows freestanding syntax checks to pass).
#ifdef NROS_CPP_STD

#include <chrono>
#include <functional>
#include <string>
#include <utility> // std::move — the closure block owns the callable

#include "nros/hosted_block.hpp"

namespace nros {

// ============================================================================
// A) std::function callback wrappers for Timer and GuardCondition
// ============================================================================
//
// Lifetime: the heap-allocated std::function lives inside a closure BLOCK,
// owned by the Timer / GuardCondition instance via
// `attach_closure_block(detail::HostedBlockBase*)`. The runtime receives a raw
// pointer to the block's `std::function` member; the Timer's destructor
// cancels the runtime callback before the block is freed, so the raw pointer
// is never dereferenced after free.
//
// The block rather than a bare `std::unique_ptr<std::function<void()>>` member
// because the OWNER's member is unconditional and cannot name a hosted type
// (issue 1225, phase-442 W1). `detail::HostedBlockBase` carries the argument.

namespace detail {

/// Trampoline that invokes a heap-allocated std::function<void()>.
inline void std_function_trampoline(void* context) {
    auto* fn = static_cast<std::function<void()>*>(context);
    (*fn)();
}

/// A `std::function<void()>` behind the unconditional closure pointer.
///
/// Carries its own destroyer, so `~Timer()` and `~GuardCondition()` — which
/// are compiled in both configurations — free it without naming it.
struct StdClosureBlock : HostedBlockBase {
    std::function<void()> fn;

    static void destroy_fn(void* p) {
        delete static_cast<StdClosureBlock*>(static_cast<HostedBlockBase*>(p));
    }

    explicit StdClosureBlock(std::function<void()> f) : fn(std::move(f)) {
        this->destroy = &StdClosureBlock::destroy_fn;
    }
};

} // namespace detail

/// A node's fully-qualified name as a `std::string` — the shape
/// `rclcpp::Node::get_fully_qualified_name` returns.
///
/// Takes the two halves rather than a node, because the case that needs it is
/// a node you DISCOVERED: `Executor::get_node_names` hands a visitor a name and
/// a namespace, and this is the step that makes them one. Neither rcl nor
/// rclcpp has a counterpart — their graph APIs return two parallel arrays and
/// leave both the correlation and the join to the caller.
///
/// Returns an empty string if the join cannot be represented, which for a name
/// and namespace that came from the graph cannot happen; a caller that builds
/// its own inputs should use the `Result`-returning C entry point.
inline std::string get_fully_qualified_name(const char* node_name, const char* node_namespace) {
    char buf[256];
    size_t len = 0;
    if (nros_get_fully_qualified_name(node_name, node_namespace, buf, sizeof(buf), &len) != 0) {
        return std::string();
    }
    return std::string(buf, len);
}

/// Create a repeating timer with a std::function callback.
///
/// The closure is owned by the `Timer` — it is freed automatically
/// when the `Timer` is destroyed or moved-from. No manual lifetime
/// management required.
///
/// @param node      The parent node.
/// @param out       Receives the initialized timer.
/// @param period    Timer period.
/// @param callback  Callable invoked on each tick.
/// @return Result indicating success or failure.
inline Result create_wall_timer(::rclcpp::Node& node, Timer& out, std::chrono::milliseconds period,
                                std::function<void()> callback) {
    auto* block = new detail::StdClosureBlock(std::move(callback));
    Result r = node.create_wall_timer(out, static_cast<uint64_t>(period.count()),
                                      detail::std_function_trampoline, &block->fn);
    if (!r.ok()) {
        delete block;
        return r;
    }
    out.attach_closure_block(static_cast<detail::HostedBlockBase*>(block));
    return r;
}

/// Create a repeating timer on a CLOCK, with a std::function callback —
/// `rclcpp::create_timer(node, clock, period, callback)` (phase-425 W4).
///
/// The clock-taking counterpart of `create_wall_timer` above: a
/// `NROS_CLOCK_ROS_TIME` clock makes the timer follow `/clock`, so it pauses
/// with the simulator and tracks a bag's replay rate.
///
/// Same ownership rules as `create_wall_timer`: the closure lives with the
/// Timer and is freed on destruction.
///
/// @param node      The parent node.
/// @param out       Receives the initialized timer.
/// @param clock     The clock that advances the timer.
/// @param period    Timer period, ON THAT CLOCK.
/// @param callback  Callable invoked on each tick.
/// @return Result indicating success or failure.
inline Result create_timer(::rclcpp::Node& node, Timer& out, const Clock& clock,
                           std::chrono::milliseconds period, std::function<void()> callback) {
    auto* block = new detail::StdClosureBlock(std::move(callback));
    Result r = node.create_timer(out, clock, static_cast<uint64_t>(period.count()),
                                 detail::std_function_trampoline, &block->fn);
    if (!r.ok()) {
        delete block;
        return r;
    }
    out.attach_closure_block(static_cast<detail::HostedBlockBase*>(block));
    return r;
}

/// Create a one-shot timer with a std::function callback.
///
/// Same ownership rules as `create_wall_timer`: the closure lives with the
/// Timer and is freed on destruction.
inline Result create_timer_oneshot(::rclcpp::Node& node, Timer& out,
                                   std::chrono::milliseconds delay,
                                   std::function<void()> callback) {
    auto* block = new detail::StdClosureBlock(std::move(callback));
    Result r = node.create_timer_oneshot(out, static_cast<uint64_t>(delay.count()),
                                         detail::std_function_trampoline, &block->fn);
    if (!r.ok()) {
        delete block;
        return r;
    }
    out.attach_closure_block(static_cast<detail::HostedBlockBase*>(block));
    return r;
}

/// Create a guard condition with a std::function callback.
///
/// Same ownership rules as `create_wall_timer`.
inline Result create_guard_condition(::rclcpp::Node& node, GuardCondition& out,
                                     std::function<void()> callback) {
    auto* block = new detail::StdClosureBlock(std::move(callback));
    Result r = node.create_guard_condition(out, detail::std_function_trampoline, &block->fn);
    if (!r.ok()) {
        delete block;
        return r;
    }
    out.attach_closure_block(static_cast<detail::HostedBlockBase*>(block));
    return r;
}

// ============================================================================
// B) std::string forwarding overloads
// ============================================================================

/// Initialize an nros session (std::string overload).
inline Result init(const std::string& locator, uint8_t domain_id = 0) {
    return init(locator.c_str(), domain_id);
}

/// Create a node (std::string overload).
inline Result create_node(::rclcpp::Node& out, const std::string& name,
                          const std::string& ns = std::string()) {
    return create_node(out, name.c_str(), ns.empty() ? nullptr : ns.c_str());
}

// -- Node member std::string overloads (free functions that forward) --

/// Create a publisher (std::string topic overload).
template <typename M>
Result create_publisher(::rclcpp::Node& node, Publisher<M>& out, const std::string& topic,
                        const QoS& qos = QoS::default_profile()) {
    return node.create_publisher(out, topic.c_str(), qos);
}

/// Create a subscription (std::string topic overload).
template <typename M>
Result create_subscription(::rclcpp::Node& node, Subscription<M>& out, const std::string& topic,
                           const QoS& qos = QoS::default_profile()) {
    return node.create_subscription(out, topic.c_str(), qos);
}

/// Create a service server (std::string name overload).
template <typename S>
Result create_service(::rclcpp::Node& node, Service<S>& out, const std::string& service_name,
                      const QoS& qos = QoS::services()) {
    return node.create_service(out, service_name.c_str(), qos);
}

/// Create a service client (std::string name overload).
template <typename S>
Result create_client(::rclcpp::Node& node, Client<S>& out, const std::string& service_name,
                     const QoS& qos = QoS::services()) {
    return node.create_client(out, service_name.c_str(), qos);
}

/// Create an action server (std::string name overload).
template <typename A>
Result create_action_server(::rclcpp::Node& node, ActionServer<A>& out,
                            const std::string& action_name, const QoS& qos = QoS::services()) {
    return node.create_action_server(out, action_name.c_str(), qos);
}

/// Create an action client (std::string name overload).
template <typename A>
Result create_action_client(::rclcpp::Node& node, ActionClient<A>& out,
                            const std::string& action_name, const QoS& qos = QoS::services()) {
    return node.create_action_client(out, action_name.c_str(), qos);
}

// -- Executor std::string overloads --

/// Create an executor (std::string overload).
inline Result create_executor(Executor& out, const std::string& locator, uint8_t domain_id = 0) {
    return Executor::create(out, locator.c_str(), domain_id);
}

/// Create a node on an executor (std::string overload).
inline Result create_node(Executor& exec, ::rclcpp::Node& out, const std::string& name,
                          const std::string& ns = std::string()) {
    return exec.create_node(out, name.c_str(), ns.empty() ? nullptr : ns.c_str());
}

// ============================================================================
// C) std::chrono duration overloads
// ============================================================================

/// Drive transport I/O (std::chrono overload).
inline Result spin_once(std::chrono::milliseconds timeout) {
    return spin_once(static_cast<int32_t>(timeout.count()));
}

/// Spin for a duration (std::chrono overload).
inline Result spin(std::chrono::milliseconds duration,
                   std::chrono::milliseconds poll = std::chrono::milliseconds(10)) {
    return spin(static_cast<uint32_t>(duration.count()), static_cast<int32_t>(poll.count()));
}

} // namespace nros

// -- Executor std::chrono member-like free functions --

namespace nros {

/// Executor spin_once (std::chrono overload, free function).
inline Result executor_spin_once(Executor& exec, std::chrono::milliseconds timeout) {
    return exec.spin_once(static_cast<int32_t>(timeout.count()));
}

/// Executor bounded spin (std::chrono overload, free function).
///
/// Issue 0338 — forwards to `spin_for`, the bounded verb; `Executor::spin()`
/// now blocks until shutdown, as in rclcpp.
inline Result executor_spin_for(Executor& exec, std::chrono::milliseconds duration,
                                std::chrono::milliseconds poll = std::chrono::milliseconds(10)) {
    return exec.spin_for(static_cast<uint32_t>(duration.count()),
                         static_cast<int32_t>(poll.count()));
}

/// Deprecated alias for [`executor_spin_for`] — issue 0338.
[[deprecated("bounded spin is now `executor_spin_for(...)` (issue 0338)")]] inline Result
executor_spin(Executor& exec, std::chrono::milliseconds duration,
              std::chrono::milliseconds poll = std::chrono::milliseconds(10)) {
    return executor_spin_for(exec, duration, poll);
}

} // namespace nros

#endif // NROS_CPP_STD

#endif // NROS_CPP_STD_COMPAT_HPP
