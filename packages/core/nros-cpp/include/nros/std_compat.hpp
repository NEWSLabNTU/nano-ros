// nros-cpp: Optional std mode conveniences
// Requires hosted C++ with STL — guarded by NROS_CPP_STD
//
// Provides:
// A) std::function<void()> callback wrappers for Timer and GuardCondition
// B) std::string forwarding overloads for name parameters
// C) std::chrono::milliseconds overloads for spin/timer durations

#ifndef NROS_CPP_STD_COMPAT_HPP
#define NROS_CPP_STD_COMPAT_HPP

// This header is a no-op without NROS_CPP_STD (allows freestanding syntax checks to pass).
#ifdef NROS_CPP_STD

#include <chrono>
#include <functional>
#include <string>

namespace nros {

// ============================================================================
// A) std::function callback wrappers for Timer and GuardCondition
// ============================================================================

namespace detail {

/// Trampoline that invokes a heap-allocated std::function<void()>.
inline void std_function_trampoline(void* context) {
    auto* fn = static_cast<std::function<void()>*>(context);
    (*fn)();
}

} // namespace detail

/// Create a repeating timer with a std::function callback.
///
/// The `callback` is heap-allocated and must outlive the Timer. The caller
/// is responsible for calling `delete` on the returned pointer when the
/// Timer is destroyed, or managing lifetime via shared_ptr capture.
///
/// @param node      The parent node.
/// @param out       Receives the initialized timer.
/// @param period    Timer period.
/// @param callback  Callable invoked on each tick.
/// @return Result indicating success or failure.
inline Result create_timer(Node& node, Timer& out, std::chrono::milliseconds period,
                           std::function<void()> callback) {
    auto* fn = new std::function<void()>(std::move(callback));
    return node.create_timer(out, static_cast<uint64_t>(period.count()),
                             detail::std_function_trampoline, fn);
}

/// Create a one-shot timer with a std::function callback.
///
/// Same lifetime rules as create_timer().
///
/// @param node      The parent node.
/// @param out       Receives the initialized timer.
/// @param delay     Delay before the callback fires.
/// @param callback  Callable invoked once.
/// @return Result indicating success or failure.
inline Result create_timer_oneshot(Node& node, Timer& out, std::chrono::milliseconds delay,
                                   std::function<void()> callback) {
    auto* fn = new std::function<void()>(std::move(callback));
    return node.create_timer_oneshot(out, static_cast<uint64_t>(delay.count()),
                                     detail::std_function_trampoline, fn);
}

/// Create a guard condition with a std::function callback.
///
/// Same lifetime rules as create_timer().
///
/// @param node      The parent node.
/// @param out       Receives the initialized guard condition.
/// @param callback  Callable invoked when triggered.
/// @return Result indicating success or failure.
inline Result create_guard_condition(Node& node, GuardCondition& out,
                                     std::function<void()> callback) {
    auto* fn = new std::function<void()>(std::move(callback));
    return node.create_guard_condition(out, detail::std_function_trampoline, fn);
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
