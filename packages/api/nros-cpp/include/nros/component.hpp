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

#include "nros/action_client.hpp" // action-client set_callbacks + goal/feedback/result typedefs
#include "nros/action_server.hpp" // raw action-server register + set_callbacks + storage size
#include "nros/node.hpp"
#include "nros/result.hpp"
#include "nros/service.hpp"      // nros_cpp_service_server_register (raw callback)
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
    f.tx_express = qos.tx_express() ? 1 : 0;
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
                                                        ctx, /*sched_context=*/0, &handle,
                                                        /*callback_group=*/nullptr);
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

/// Phase 242.2 (RFC-0044 §Design.2(1)) — bind a component **member**
/// `void C::on_msg(const M&)` as a **typed** subscription callback. This is the
/// `bind_subscription_raw` no-alloc trampoline lifted to the typed path: it
/// registers a RAW subscription (so the executor arena owns it, no C++
/// `Subscription<M>` storage object) keyed on the DDS-mangled `M::TYPE_NAME`
/// (the same wire keyexpr a typed `Publisher<M>` registers — the 240.1 finding,
/// RFC-0044 Q4), and the trampoline `M::ffi_deserialize`s the wire bytes into a
/// stack `M` before dispatching to the typed member. `self` is the executor
/// `ctx`; the member-fn pointer is a template parameter, so the trampoline is a
/// non-capturing lambda that decays to a function pointer — no heap, no
/// `std::function`.
///
/// C++14 note: `M`, `C`, and `Method` are all template parameters (none is
/// deducible from a runtime member-pointer argument without storing it). This
/// mirrors `bind_subscription_raw<C, &C::m>`; the ergonomic
/// `ComponentNode::create_subscription` member + `NROS_SUBSCRIBE` macro hide the
/// spelling.
template <typename M, class C, void (C::*Method)(const M& msg)>
inline Result bind_subscription(Node& node, const char* topic, C* self,
                                const QoS& qos = QoS::default_profile()) {
    return create_subscription_raw(
        node, topic, M::TYPE_NAME,
        [](const uint8_t* data, size_t len, void* ctx) {
            M msg;
            if (M::ffi_deserialize(data, len, &msg) != 0) return;
            (static_cast<C*>(ctx)->*Method)(msg);
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

/// Register a **raw** callback-style service server on the executor that owns
/// `node`. The handler receives the request's wire bytes (`req`, `req_len`) and
/// fills the reply into `resp` (capacity `resp_cap`), writing the byte count to
/// `*resp_len`; return `true` to send the reply, `false` to drop. `ctx` is
/// carried through. The executor owns the server; it dispatches the handler
/// during `spin_once`. (Thin wrapper over `nros_cpp_service_server_register`.)
inline Result create_service_raw(Node& node, const char* service, const char* type_name,
                                 nros_cpp_service_request_callback_t callback, void* ctx,
                                 const QoS& qos = QoS::services()) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::component_qos_to_ffi(qos);
    size_t handle = static_cast<size_t>(-1);
    nros_cpp_ret_t ret = nros_cpp_service_server_register(
        h, service, type_name, "", ffi_qos, callback, ctx, /*sched_context=*/0, &handle);
    return Result(ret);
}

/// Bind a component **member**
/// `bool C::on_request(const uint8_t* req, size_t req_len, uint8_t* resp,
///                     size_t resp_cap, size_t* resp_len)`
/// as a raw service handler. Same no-alloc member-fn-pointer-as-template-param
/// trampoline; `self` is the executor `ctx`.
template <class C, bool (C::*Method)(const uint8_t* req, size_t req_len, uint8_t* resp,
                                     size_t resp_cap, size_t* resp_len)>
inline Result bind_service_raw(Node& node, const char* service, const char* type_name, C* self,
                               const QoS& qos = QoS::services()) {
    return create_service_raw(
        node, service, type_name,
        [](const uint8_t* req, size_t req_len, uint8_t* resp, size_t resp_cap, size_t* resp_len,
           void* ctx) -> bool {
            return (static_cast<C*>(ctx)->*Method)(req, req_len, resp, resp_cap, resp_len);
        },
        self, qos);
}

/// Bind a component **member**
/// `Svc::Response C::on_request(const Svc::Request&)` as a **typed** service
/// handler — the executor-dispatched, generated-binding twin of
/// `bind_service_raw` (issue 0089 gap 4). The trampoline `ffi_deserialize`s the
/// request wire bytes into a stack `Svc::Request`, calls the typed member, and
/// `ffi_serialize`s the returned `Svc::Response` back into the reply buffer — no
/// hand-rolled CDR/alignment in the component. `Svc` is the generated service
/// type (`example_interfaces::srv::AddTwoInts`, exposing `Request` / `Response`
/// / `TYPE_NAME`); the service-type name is taken from `Svc::TYPE_NAME`, so —
/// unlike `bind_service_raw` — no `type_name` argument is needed.
///
/// C++14 note: `Svc`, `C`, and the member-pointer `Method` are template
/// parameters (mirrors `bind_subscription<M, C, &C::m>`); the trampoline is a
/// non-capturing lambda that decays to a function pointer — no heap, no
/// `std::function`. A reply larger than `resp_cap`, or a malformed request,
/// drops the reply (returns `false`).
template <typename Svc, class C, typename Svc::Response (C::*Method)(const typename Svc::Request&)>
inline Result bind_service(Node& node, const char* service, C* self,
                           const QoS& qos = QoS::services()) {
    return create_service_raw(
        node, service, Svc::TYPE_NAME,
        [](const uint8_t* req, size_t req_len, uint8_t* resp, size_t resp_cap, size_t* resp_len,
           void* ctx) -> bool {
            typename Svc::Request request{};
            if (Svc::Request::ffi_deserialize(req, req_len, &request) != 0) return false;
            typename Svc::Response response = (static_cast<C*>(ctx)->*Method)(request);
            size_t written = 0;
            if (Svc::Response::ffi_serialize(&response, resp, resp_cap, &written) != 0)
                return false;
            *resp_len = written;
            return true;
        },
        self, qos);
}

/// Storage a component must own for a raw action server (8-aligned, lives for
/// the app lifetime — the executor arena holds it). Declare one per action:
/// `::nros::ActionServerStorage fib_storage_;` then pass `fib_storage_.bytes`.
struct ActionServerStorage {
    alignas(8) uint8_t bytes[NROS_CPP_ACTION_SERVER_STORAGE_SIZE];
};

/// Register a **raw** action server on the executor that owns `node`: create →
/// register → set goal/cancel callbacks. `storage` is the component-owned buffer
/// (`ActionServerStorage::bytes`). The goal callback returns a `GoalResponse`
/// discriminant (`int32_t`; 0 reject / 1 accept-and-execute / 2 accept-defer),
/// the cancel callback a `CancelResponse`. `ctx` is carried through. After a
/// goal is accepted, complete it with `nros_cpp_action_server_complete_goal(
/// storage, node.executor_handle(), goal_id, result_cdr, len)` (and feedback via
/// `nros_cpp_action_server_publish_feedback`).
inline Result create_action_server_raw(Node& node, void* storage, const char* action_name,
                                       const char* type_name, nros_cpp_goal_callback_t goal_cb,
                                       nros_cpp_cancel_callback_t cancel_cb, void* ctx,
                                       const QoS& qos = QoS::services()) {
    const nros_cpp_node_t* h = node.ffi_handle();
    void* exec = node.executor_handle();
    if (h == nullptr || exec == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::component_qos_to_ffi(qos);
    nros_cpp_ret_t ret =
        nros_cpp_action_server_create(h, action_name, type_name, "", ffi_qos, storage);
    if (ret != 0) return Result(ret);
    ret = nros_cpp_action_server_register(storage, exec, action_name, type_name, "",
                                          /*sched_context=*/0);
    if (ret != 0) return Result(ret);
    return Result(nros_cpp_action_server_set_callbacks(storage, goal_cb, cancel_cb, ctx));
}

/// Bind component **members**
/// `int32_t C::on_goal(const uint8_t goal_id[16], const uint8_t* data, size_t len)`
/// and `int32_t C::on_cancel(const uint8_t goal_id[16])` as the action server's
/// goal/cancel callbacks (by identity, `self` as ctx, no-alloc trampolines).
template <class C,
          int32_t (C::*GoalMethod)(const uint8_t goal_id[16], const uint8_t* data, size_t len),
          int32_t (C::*CancelMethod)(const uint8_t goal_id[16])>
inline Result bind_action_server_raw(Node& node, void* storage, const char* action_name,
                                     const char* type_name, C* self,
                                     const QoS& qos = QoS::services()) {
    return create_action_server_raw(
        node, storage, action_name, type_name,
        [](const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) -> int32_t {
            return (static_cast<C*>(ctx)->*GoalMethod)(goal_id, data, len);
        },
        [](const uint8_t goal_id[16], void* ctx) -> int32_t {
            return (static_cast<C*>(ctx)->*CancelMethod)(goal_id);
        },
        self, qos);
}

/// Storage a component must own for a raw, poll-style service client (8-aligned,
/// app lifetime). Send with `nros_cpp_service_client_send_request(bytes, …)`,
/// poll with `nros_cpp_service_client_try_recv_reply(bytes, …)`.
struct ServiceClientStorage {
    alignas(8) uint8_t bytes[NROS_SERVICE_CLIENT_SIZE];
};

/// Create a raw poll-style service client into the component-owned `storage`.
inline Result create_service_client_raw(Node& node, void* storage, const char* service,
                                        const char* type_name, const QoS& qos = QoS::services()) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::component_qos_to_ffi(qos);
    return Result(nros_cpp_service_client_create(h, service, type_name, "", ffi_qos, storage));
}

/// Storage a component must own for a raw, poll-style action client.
struct ActionClientStorage {
    alignas(8) uint8_t bytes[NROS_CPP_ACTION_CLIENT_STORAGE_SIZE];
};

/// Create a raw poll-style action client into the component-owned `storage`.
/// Drive it with `nros_cpp_action_client_send_goal` /
/// `nros_cpp_action_client_try_recv_goal_response` /
/// `nros_cpp_action_client_get_result`. (Poll opt-in — for callback dispatch use
/// `bind_action_client` below.)
inline Result create_action_client_raw(Node& node, void* storage, const char* action_name,
                                       const char* type_name, const QoS& qos = QoS::services()) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::component_qos_to_ffi(qos);
    return Result(nros_cpp_action_client_create(h, action_name, type_name, "", ffi_qos, storage));
}

/// Bind a component's action client to **member callbacks** (RFC-0041 — callback
/// by default; issue-0047). `on_goal_response(bool accepted, const uint8_t
/// goal_id[16])`, `on_feedback(const uint8_t goal_id[16], const uint8_t* data,
/// size_t len)`, `on_result(const uint8_t goal_id[16], int32_t status, const
/// uint8_t* data, size_t len)` are bound by identity (no naming), `self` as ctx.
///
/// Unlike subscription/service whose RX is pumped by the session each
/// `spin_once`, the action client's goal-response/feedback/result arrive via
/// GET-query replies that must be drained with `nros_cpp_action_client_poll` —
/// which is NOT auto-called by `spin_once` (issue-0047). So this binds a
/// component-owned `poll_timer` that calls `poll()` each `poll_ms`; `poll()`
/// dispatches the buffered replies into the member callbacks. Send goals with
/// `nros_cpp_action_client_send_goal_async(storage.bytes, …)`; the acceptance
/// then arrives in `on_goal_response`.
template <class C, void (C::*OnGoalResponse)(bool accepted, const uint8_t goal_id[16]),
          void (C::*OnFeedback)(const uint8_t goal_id[16], const uint8_t* data, size_t len),
          void (C::*OnResult)(const uint8_t goal_id[16], int32_t status, const uint8_t* data,
                              size_t len)>
inline Result bind_action_client(Node& node, ActionClientStorage& storage, Timer& poll_timer,
                                 const char* action_name, const char* type_name, C* self,
                                 uint64_t poll_ms = 20, const QoS& qos = QoS::services()) {
    Result r = create_action_client_raw(node, storage.bytes, action_name, type_name, qos);
    if (!r.ok()) return r;
    nros_cpp_ret_t ret = nros_cpp_action_client_set_callbacks(
        storage.bytes,
        [](bool accepted, const uint8_t goal_id[16], void* ctx) {
            (static_cast<C*>(ctx)->*OnGoalResponse)(accepted, goal_id);
        },
        [](const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) {
            (static_cast<C*>(ctx)->*OnFeedback)(goal_id, data, len);
        },
        [](const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len, void* ctx) {
            (static_cast<C*>(ctx)->*OnResult)(goal_id, status, data, len);
        },
        self);
    if (ret != 0) return Result(ret);
    // Pump the GET-query replies each spin tick → callbacks fire from poll().
    return node.create_timer(
        poll_timer, poll_ms, [](void* ctx) { nros_cpp_action_client_poll(ctx); }, storage.bytes);
}

} // namespace nros

/// Convenience: bind a component subscription member without spelling the
/// template arguments. `Msg` is unused at runtime (the raw path is type-erased on
/// the wire) but documents the topic's type; pass the ROS type-name string.
#define NROS_BIND_SUB_RAW(node, Class, method, topic, type_name, self)                             \
    ::nros::bind_subscription_raw<Class, &Class::method>((node), (topic), (type_name), (self))

/// Convenience: bind a **typed** component subscription member
/// `void Class::method(const Msg&)` without spelling the template arguments.
/// `Msg::TYPE_NAME` (the DDS-mangled keyexpr) is registered automatically.
#define NROS_BIND_SUB(node, Msg, Class, method, topic, self)                                       \
    ::nros::bind_subscription<Msg, Class, &Class::method>((node), (topic), (self))

/// Convenience: bind a component timer member.
#define NROS_BIND_TIMER(node, Class, method, out, period_ms, self)                                 \
    ::nros::bind_timer<Class, &Class::method>((node), (out), (period_ms), (self))

#endif // NROS_COMPONENT_HPP
