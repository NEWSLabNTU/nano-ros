/// @file component.hpp
/// @brief Phase 240.1 (RFC-0043) — stateful component-object binding helpers.
///
/// The declarative Entry path used to record string descriptors + a
/// synthesizing interpreter (`EntryNodeRuntime`). RFC-0043 routes it to the real
/// executor instead: a component is a **stateful object** that binds its real
/// callbacks **by identity** (no string names), and `spin_once` dispatches them.
///
/// The typed callback-style `Node::create_subscription(sub, topic, fn)` is
/// **stateless** (`void(const M&)`, no ctx) — useless for a component that
/// mutates its own state. The helpers here bind a **member function** of the
/// component as the callback, with the component pointer carried as the executor
/// `ctx` and a compile-time-generated (no-alloc) trampoline:
///
/// ```cpp
/// class Talker {
///     nros::Publisher<Int32> pub_;
///     nros::Timer timer_;
///     int count_ = 0;
///     void on_tick() { Int32 m; m.data = count_++; pub_.publish(m); }  // real body
///   public:
///     nros::Result configure(nros::Node& node) {
///         NROS_TRY(node.create_publisher(pub_, "/chatter"));
///         return nros::bind_timer<Talker, &Talker::on_tick>(node, timer_, 1000, this);
///     }
/// };
/// ```
///
/// No callback name anywhere — the binding is the member-function pointer itself.

#ifndef NROS_COMPONENT_HPP
#define NROS_COMPONENT_HPP

#include <cstddef>
#include <cstdint>

#include "nros/node.hpp"
#include "nros/result.hpp"
#include "nros/subscription.hpp" // nros_cpp_subscription_register (raw callback)

namespace nros {

namespace detail {
/// QoS → FFI struct (the 9-field copy `Node::create_subscription` does inline).
inline nros_cpp_qos_t component_qos_to_ffi(const QoS& qos) {
    nros_cpp_qos_t f;
    f.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
    f.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
    f.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
    f.liveliness_kind = static_cast<nros_cpp_qos_liveliness_t>(qos.liveliness_raw());
    f.depth = qos.depth();
    f.deadline_ms = qos.deadline_ms();
    f.lifespan_ms = qos.lifespan_ms();
    f.liveliness_lease_ms = qos.liveliness_lease_ms();
    f.avoid_ros_namespace_conventions = qos.avoid_ros_namespace_conventions() ? 1 : 0;
    return f;
}
} // namespace detail

/// Register a **raw, zero-copy** subscription on the executor: the callback
/// borrows the wire bytes (`data`, `len`) directly — no copy, no deserialize, no
/// typed header. `ctx` is carried through to the callback. The executor owns the
/// subscription (no storage object needed on the caller side); it dispatches the
/// callback during `spin_once`. (Thin wrapper over `nros_cpp_subscription_register`.)
inline Result create_subscription_raw(Node& node, const char* topic, const char* type_name,
                                      void (*callback)(const uint8_t* data, size_t len, void* ctx),
                                      void* ctx, const QoS& qos = QoS::default_profile()) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::component_qos_to_ffi(qos);
    size_t handle = static_cast<size_t>(-1);
    nros_cpp_ret_t ret = nros_cpp_subscription_register(h, topic, type_name, "", ffi_qos, callback,
                                                        ctx, /*sched_context=*/0, &handle);
    return Result(ret);
}

/// Bind a component **member** `void C::on_msg(const uint8_t*, size_t)` as a raw
/// (zero-copy) subscription callback. The member-fn pointer is a template
/// parameter, so the trampoline is a non-capturing lambda (decays to a function
/// pointer — no heap, no `std::function`). `self` is the executor `ctx`.
template <class C, void (C::*Method)(const uint8_t* data, size_t len)>
inline Result bind_subscription_raw(Node& node, const char* topic, const char* type_name, C* self,
                                    const QoS& qos = QoS::default_profile()) {
    return create_subscription_raw(
        node, topic, type_name,
        [](const uint8_t* data, size_t len, void* ctx) {
            (static_cast<C*>(ctx)->*Method)(data, len);
        },
        self, qos);
}

/// Bind a component **member** `void C::on_tick()` as a timer callback. Same
/// no-alloc member-pointer-as-template-param trampoline; `self` is the ctx the
/// executor hands back. Wraps the existing `Node::create_timer(out, ms, cb, ctx)`.
template <class C, void (C::*Method)()>
inline Result bind_timer(Node& node, Timer& out, uint64_t period_ms, C* self) {
    return node.create_timer(
        out, period_ms, [](void* ctx) { (static_cast<C*>(ctx)->*Method)(); }, self);
}

} // namespace nros

/// Convenience: bind a component subscription member without spelling the
/// template arguments. `Msg` is unused at runtime (the raw path is type-erased on
/// the wire) but documents the topic's type; pass the ROS type-name string.
#define NROS_BIND_SUB_RAW(node, Class, method, topic, type_name, self)                             \
    ::nros::bind_subscription_raw<Class, &Class::method>((node), (topic), (type_name), (self))

/// Convenience: bind a component timer member.
#define NROS_BIND_TIMER(node, Class, method, out, period_ms, self)                                 \
    ::nros::bind_timer<Class, &Class::method>((node), (out), (period_ms), (self))

#endif // NROS_COMPONENT_HPP
