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
#include <memory>
#include <string>

namespace nros {

// ============================================================================
// A) std::function callback wrappers for Timer and GuardCondition
// ============================================================================
//
// Lifetime: the heap-allocated std::function is owned by the Timer /
// GuardCondition instance via `attach_std_closure(unique_ptr)`. The
// runtime receives a raw pointer into the same std::function; the
// Timer's destructor cancels the runtime callback before the
// unique_ptr is dropped, so the raw pointer is never dereferenced
// after free.

namespace detail {

/// Trampoline that invokes a heap-allocated std::function<void()>.
inline void std_function_trampoline(void* context) {
    auto* fn = static_cast<std::function<void()>*>(context);
    (*fn)();
}

} // namespace detail

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
inline Result create_timer(Node& node, Timer& out, std::chrono::milliseconds period,
                           std::function<void()> callback) {
    auto fn =
        std::unique_ptr<std::function<void()>>(new std::function<void()>(std::move(callback)));
    auto* raw = fn.get();
    Result r = node.create_timer(out, static_cast<uint64_t>(period.count()),
                                 detail::std_function_trampoline, raw);
    if (r.ok()) {
        out.attach_std_closure(std::move(fn));
    }
    return r;
}

/// Create a one-shot timer with a std::function callback.
///
/// Same ownership rules as `create_timer`: the closure lives with the
/// Timer and is freed on destruction.
inline Result create_timer_oneshot(Node& node, Timer& out, std::chrono::milliseconds delay,
                                   std::function<void()> callback) {
    auto fn =
        std::unique_ptr<std::function<void()>>(new std::function<void()>(std::move(callback)));
    auto* raw = fn.get();
    Result r = node.create_timer_oneshot(out, static_cast<uint64_t>(delay.count()),
                                         detail::std_function_trampoline, raw);
    if (r.ok()) {
        out.attach_std_closure(std::move(fn));
    }
    return r;
}

/// Create a guard condition with a std::function callback.
///
/// Same ownership rules as `create_timer`.
inline Result create_guard_condition(Node& node, GuardCondition& out,
                                     std::function<void()> callback) {
    auto fn =
        std::unique_ptr<std::function<void()>>(new std::function<void()>(std::move(callback)));
    auto* raw = fn.get();
    Result r = node.create_guard_condition(out, detail::std_function_trampoline, raw);
    if (r.ok()) {
        out.attach_std_closure(std::move(fn));
    }
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
inline Result create_node(Node& out, const std::string& name,
                          const std::string& ns = std::string()) {
    return create_node(out, name.c_str(), ns.empty() ? nullptr : ns.c_str());
}

// -- Node member std::string overloads (free functions that forward) --

/// Create a publisher (std::string topic overload).
template <typename M>
Result create_publisher(Node& node, Publisher<M>& out, const std::string& topic,
                        const QoS& qos = QoS::default_profile()) {
    return node.create_publisher(out, topic.c_str(), qos);
}

/// Create a subscription (std::string topic overload).
template <typename M>
Result create_subscription(Node& node, Subscription<M>& out, const std::string& topic,
                           const QoS& qos = QoS::default_profile()) {
    return node.create_subscription(out, topic.c_str(), qos);
}

/// Create a service server (std::string name overload).
template <typename S>
Result create_service(Node& node, Service<S>& out, const std::string& service_name,
                      const QoS& qos = QoS::services()) {
    return node.create_service(out, service_name.c_str(), qos);
}

/// Create a service client (std::string name overload).
template <typename S>
Result create_client(Node& node, Client<S>& out, const std::string& service_name,
                     const QoS& qos = QoS::services()) {
    return node.create_client(out, service_name.c_str(), qos);
}

/// Create an action server (std::string name overload).
template <typename A>
Result create_action_server(Node& node, ActionServer<A>& out, const std::string& action_name,
                            const QoS& qos = QoS::services()) {
    return node.create_action_server(out, action_name.c_str(), qos);
}

/// Create an action client (std::string name overload).
template <typename A>
Result create_action_client(Node& node, ActionClient<A>& out, const std::string& action_name,
                            const QoS& qos = QoS::services()) {
    return node.create_action_client(out, action_name.c_str(), qos);
}

// -- Executor std::string overloads --

/// Create an executor (std::string overload).
inline Result create_executor(Executor& out, const std::string& locator, uint8_t domain_id = 0) {
    return Executor::create(out, locator.c_str(), domain_id);
}

/// Create a node on an executor (std::string overload).
inline Result create_node(Executor& exec, Node& out, const std::string& name,
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

/// Executor spin (std::chrono overload, free function).
inline Result executor_spin(Executor& exec, std::chrono::milliseconds duration,
                            std::chrono::milliseconds poll = std::chrono::milliseconds(10)) {
    return exec.spin(static_cast<uint32_t>(duration.count()), static_cast<int32_t>(poll.count()));
}

} // namespace nros

#endif // NROS_CPP_STD

#endif // NROS_CPP_STD_COMPAT_HPP
