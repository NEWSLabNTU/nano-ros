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
///         return node.create_wall_timer<Talker, &Talker::on_tick>(timer_, 1000, this);
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
#include "nros/size_bound.hpp"   // nros::rx_size_bound<M> — the derived receive bound
#include "nros/subscription.hpp" // nros_cpp_subscription_register (raw callback)

namespace nros {

/// Register a **raw, zero-copy** subscription on the executor: the callback
/// borrows the wire bytes (`data`, `len`) directly — no copy, no deserialize, no
/// typed header. `ctx` is carried through to the callback. The executor owns the
/// subscription (no storage object needed on the caller side); it dispatches the
/// callback during `spin_once`. (Thin wrapper over `nros_cpp_subscription_register`.)
inline Result create_subscription_raw(Node& node, const char* topic, const char* type_name,
                                      void (*callback)(const uint8_t* data, size_t len, void* ctx),
                                      void* ctx, const QoS& qos = QoS::default_profile(),
                                      size_t rx_bytes = 0) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
    size_t handle = static_cast<size_t>(-1);
    // phase-403 W3 -- `rx_bytes` is the subscribed type's own bound. It sizes the
    // executor's arena slot for this subscription, so a publisher-heavy image
    // stops charging every slot the largest subscription's buffer.
    //
    // 0 keeps the pre-phase-403 behaviour (the image-wide default), and is what
    // a caller with no type in hand passes. Options are stack-local: the FFI
    // reads the struct during the call and retains nothing.
    nros_cpp_subscription_options_t opts = {};
    opts.rx_buffer_hint = static_cast<uint32_t>(rx_bytes);
    const nros_cpp_subscription_options_t* opts_p = (rx_bytes != 0) ? &opts : nullptr;
    nros_cpp_ret_t ret = nros_cpp_subscription_register(h, topic, type_name, "", ffi_qos, callback,
                                                        ctx, &handle, opts_p);
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
/// `Node::create_subscription_in` member + `NROS_SUBSCRIBE` macro hide the
/// spelling.
template <typename M, class C, void (C::*Method)(const M& msg)>
inline Result bind_subscription(Node& node, const char* topic, C* self,
                                const QoS& qos = QoS::default_profile()) {
    // phase-403 W3 -- `M` is still in scope here, so the type's own bound can be
    // spent on the arena slot. This is the point the type is erased: everything
    // below takes a type NAME, and the bound cannot be recovered from a string.
    //
    // phase-408 W1/W4 -- the number is `nros::rx_size_bound<M>`, the DERIVED
    // bound the C++ pack now emits (`M::RX_MAX_SERIALIZED_SIZE`, from
    // `nros_serdes::size::max_serialized_size`), and NOT `M::SERIALIZED_SIZE_MAX`
    // as this line read until now. That one ESTIMATES -- flat 512 per nested
    // message, flat default capacity per string -- and is wrong in BOTH
    // directions: `geometry_msgs/Twist` reads 1028 against a derived 64, while a
    // nested type larger than 512 comes out SMALLER than its real bound, which
    // under-sizes the receive buffer and drops samples silently (issues
    // 0896/0964). Over 120 stock Humble types the estimate matched the derived
    // bound zero times.
    return create_subscription_raw(
        node, topic, M::TYPE_NAME,
        [](const uint8_t* data, size_t len, void* ctx) {
            M msg;
            if (M::ffi_deserialize(data, len, &msg) != 0) return;
            (static_cast<C*>(ctx)->*Method)(msg);
        },
        self, qos, ::nros::rx_size_bound<M>::value);
}

/// `bind_subscription` with the receive-buffer hint supplied by the CALLER.
///
/// The escape hatch for a type with no derived bound (an unbounded `string` or
/// sequence in the `.msg`), where `bind_subscription` is a deliberate compile
/// error naming the offending member: there is no number the generated header
/// could pass, and choosing one is a decision only the caller can make. Also
/// the way to deliberately override a bounded type's own number.
///
/// `rx_bytes` is a HINT (0 = the image-wide default), never a promise that
/// larger samples are refused. The C sibling of this is the generated
/// `{Msg}_subscribe_sized` macro.
template <typename M, class C, void (C::*Method)(const M& msg)>
inline Result bind_subscription_sized(Node& node, const char* topic, C* self, size_t rx_bytes,
                                      const QoS& qos = QoS::default_profile()) {
    return create_subscription_raw(
        node, topic, M::TYPE_NAME,
        [](const uint8_t* data, size_t len, void* ctx) {
            M msg;
            if (M::ffi_deserialize(data, len, &msg) != 0) return;
            (static_cast<C*>(ctx)->*Method)(msg);
        },
        self, qos, rx_bytes);
}

/// **RETIRED — phase-427 W3.** Write
/// `node.create_wall_timer<C, &C::method>(out, period_ms, self)` instead.
///
/// This was an INVENTED free-function name doing exactly what upstream's
/// `create_wall_timer` does, with the node as its first argument instead of its
/// receiver. Folding it into the member removed an invention by reusing an
/// upstream name, which is what RFC-0089's clause 2 asks for — and the member
/// is the verb a reader meets first, so a component no longer has to learn a
/// second spelling for its most common line.
///
/// Kept as a deprecated forwarder for one release rather than deleted: the
/// in-tree call sites all moved in the same commit, so the attribute costs
/// nothing here and gives an out-of-tree consumer the migration in the
/// compiler's own words. There is no second code path — it forwards.
template <class C, void (C::*Method)()>
NROS_CPP_DEPRECATED_MSG("nros::bind_timer is retired (phase-427 W3): write "
                        "node.create_wall_timer<C, &C::method>(out, period_ms, self)")
inline Result bind_timer(Node& node, Timer& out, uint64_t period_ms, C* self) {
    return node.template create_wall_timer<C, Method>(out, period_ms, self);
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
    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
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
    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
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
/// poll with `nros_cpp_service_client_take_response(bytes, …)`.
struct ServiceClientStorage {
    alignas(8) uint8_t bytes[NROS_SERVICE_CLIENT_SIZE];
};

/// Create a raw poll-style service client into the component-owned `storage`.
inline Result create_service_client_raw(Node& node, void* storage, const char* service,
                                        const char* type_name, const QoS& qos = QoS::services()) {
    const nros_cpp_node_t* h = node.ffi_handle();
    if (h == nullptr) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
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
    nros_cpp_qos_t ffi_qos = detail::qos_to_ffi(qos);
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
    return node.create_wall_timer(
        poll_timer, poll_ms, [](void* ctx) { nros_cpp_action_client_poll(ctx); }, storage.bytes);
}

// ==== phase-427 W4 — Node's member-pointer subscription family ==============
//
// DECLARED in `node.hpp` (on `class Node`), DEFINED here. `node.hpp` cannot
// hold these bodies: they call `nros::bind_subscription` above, and this file
// includes `node.hpp`, so a definition there would close an include cycle. The
// umbrella `nros.hpp` pulls this file in, so the definitions are visible
// wherever the members are reachable.
//
// Both came off `nros::ComponentNode`. See the rename note on
// `Node::create_publisher_in` for why a bare `create_subscription` carrying an
// ours-only signature is the one thing the merge could not ship.
//
// The two suffixes are NOT the same word. `_in` is the ours-only/storage-free
// shape; `_in_group` takes a `CallbackGroup` first and is RFC-0047's binding.
// The second one below is both — a group form of the storage-free shape — and
// its name says the half a reader cannot infer from the argument list.

} // namespace nros

namespace rclcpp {
template <typename M, class C, void (C::*Method)(const M& msg)>
inline void Node::create_subscription_in(const char* topic, const ::nros::QoS& qos) {
    if (!this->check_declared_depth(M::TYPE_NAME, topic, qos)) {
        return;
    }
    Result r = ::nros::bind_subscription<M, C, Method>(*this, topic, static_cast<C*>(this), qos);
    if (!r.ok()) {
        this->set_error("create_subscription_in", r.raw());
    }
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename M, class C, void (C::*Method)(const M& msg)>
inline void Node::create_subscription_in_group(const ::nros::CallbackGroup& group,
                                               const char* topic, const ::nros::QoS& qos) {
    // phase-403 step 2 — the same boot-time check as the ungrouped form. A
    // grouped subscription costs the arena exactly what an ungrouped one does,
    // so leaving this path out would make the declared depth enforceable
    // everywhere except in the images that use callback groups.
    if (!this->check_declared_depth(M::TYPE_NAME, topic, qos)) {
        return;
    }
    const nros_cpp_node_t* h = this->ffi_handle();
    if (h == nullptr) {
        this->set_error("create_subscription_in_group", -3);
        return;
    }
    nros_cpp_qos_t ffi_qos = ::nros::detail::qos_to_ffi(qos);
    C* self = static_cast<C*>(this);
    size_t handle = static_cast<size_t>(-1);
    // phase-402: the group name is a FIELD now, not a trailing argument. Start
    // from the library's defaults so a future field cannot be left as whatever
    // this frame happened to hold.
    nros_cpp_subscription_options_t sub_options = nros_cpp_subscription_default_options();
    sub_options.callback_group = group.get_name();
    nros_cpp_ret_t ret = nros_cpp_subscription_register(
        h, topic, M::TYPE_NAME, "", ffi_qos,
        [](const uint8_t* data, size_t len, void* ctx) {
            M msg;
            if (M::ffi_deserialize(data, len, &msg) != 0) return;
            (static_cast<C*>(ctx)->*Method)(msg);
        },
        self, &handle, &sub_options);
    if (ret != 0) {
        this->set_error("create_subscription_in_group", ret);
    }
}
} // namespace rclcpp

namespace nros {

namespace detail {

/// Strips the reference `decltype(*this)` yields, so `&Self::method` is
/// well-formed inside the ergonomic macros below.
template <class T> struct strip_ref {
    using type = T;
};
template <class T> struct strip_ref<T&> {
    using type = T;
};

/// phase-403 step 2 — the QoS a `NROS_SUBSCRIBE` with no QoS argument gets.
///
/// `QoS(N)` is `QoS::default_profile()` with the depth replaced (RELIABLE,
/// VOLATILE, KEEP_LAST), so filling the declared depth in changes exactly the
/// one field the declaration spoke about and nothing else.
///
/// `DECLARED_DEPTH_UNDECLARED` returns the default profile untouched. That is
/// the back-compatibility hinge: every call site that existed before this step,
/// in an image with no declared depth anywhere, gets the same depth-10 profile
/// it always got, from a `constexpr` branch the optimiser folds away.
constexpr ::nros::QoS qos_from_declared_depth(int declared) {
    return (declared == ::nros::DECLARED_DEPTH_UNDECLARED) ? ::nros::QoS::default_profile()
                                                           : ::nros::QoS(declared);
}

} // namespace detail

} // namespace nros

// Zephyr's minimal libcpp ships a STUB <new> (guard
// ZEPHYR_SUBSYS_CPP_INCLUDE_NEW_) that declares nothrow_t but NOT placement
// new — the NROS_COMPONENT factory's `new (storage) Class(...)` then fails
// "no matching operator new(sizetype, void*&)" (first hit porting real
// Autoware components to a Zephyr image; ASI's FVP build used a full-libcpp
// toolchain and never saw it). Provide the standard non-allocating forms.
#ifdef ZEPHYR_SUBSYS_CPP_INCLUDE_NEW_
inline void* operator new(::std::size_t, void* ptr) noexcept {
    return ptr;
}
inline void* operator new[](::std::size_t, void* ptr) noexcept {
    return ptr;
}
inline void operator delete(void*, void*) noexcept {}
inline void operator delete[](void*, void*) noexcept {}
#endif

/// Inside a node constructor: subscribe `void Self::method(const Msg&)` to
/// `topic`. Derives `Self` from `this` so only the message type, method, and
/// topic are spelled. An optional 4th argument is the QoS.
///
/// State the depth on a memory-constrained target. Since phase-403 a
/// subscription's arena buffer is sized from its own type, so the cost of a
/// subscription is `(depth + 1) * the type's bound` -- depth is a multiplier on
/// the largest thing the topic carries, not a count of small slots. Measured on
/// mr-canhubk344: nine subscriptions at the default depth 10 wanted 86108 bytes
/// of arena, and the same nine at depth 1 want 24516.
///
/// # phase-403 step 2 — the depth is DECLARED, and the two must agree
///
/// The system's contract sidecar (`<bringup>/launch/<stem>.contract.yaml`,
/// `contracts.sub_endpoints.<endpoint>.qos.depth`) is the source of truth for
/// SIZING, because the arena is compiled before this TU exists. Both authoring
/// modes are legal:
///
///   * **the code states the QoS** — `NROS_SUBSCRIBE(M, m, "/t", nros::QoS(1))`
///     and `qos: { depth: 1 }`. If the two numbers differ, this macro fails the
///     BUILD with a `static_assert` naming the topic and both depths.
///   * **the contract states the QoS** — `NROS_SUBSCRIBE(M, m, "/t")` with
///     `qos: { depth: 1 }`, and the declared depth fills in.
///
/// With NEITHER a declaration nor a QoS the profile is `QoS::default_profile()`
/// exactly as before, and nothing is asserted: that image has not opted in, and
/// an image that has not opted in is not an image in error.
///
/// # Dispatching on ARGUMENT COUNT
///
/// C++17 has no `__VA_OPT__`, so the macro cannot branch on "a QoS was passed"
/// with the obvious spelling. `_NROS_SUB_PICK` is the standard argument-count
/// trick instead: the argument list is padded with the handler names, and which
/// one lands on the `NAME` parameter depends on how many arguments came before
/// it.
///
/// # STATEMENT context
///
/// The 4-argument form expands to a `static_assert` FOLLOWED BY the call, so it
/// is a statement and not an expression: write `NROS_SUBSCRIBE(...);` on its own
/// line, as every call site in this tree does.
#define _NROS_SUB_PICK(_1, _2, _3, _4, NAME, ...) NAME
#define NROS_SUBSCRIBE(...)                                                                        \
    _NROS_SUB_PICK(__VA_ARGS__, _NROS_SUB_4, _NROS_SUB_3, _NROS_SUB_TOO_FEW, _NROS_SUB_TOO_FEW)    \
    (__VA_ARGS__)

/// The 3-argument form: no QoS at the call site, so the DECLARED depth supplies
/// one. Nothing to assert — there is only ever one number.
#define _NROS_SUB_3(Msg, method, topic)                                                            \
    this->template create_subscription_in<                                                         \
        Msg, ::nros::detail::strip_ref<decltype(*this)>::type,                                     \
        &::nros::detail::strip_ref<decltype(*this)>::type::method>(                                \
        (topic),                                                                                   \
        ::nros::detail::qos_from_declared_depth(::nros::declared_depth(Msg::TYPE_NAME, (topic))))

/// The 4-argument form: the call site states a QoS, so the two numbers must
/// agree and the BUILD is where that is settled. `#topic` is the topic as the
/// call site wrote it — the only way a string can reach a `static_assert`
/// message in C++17.
#define _NROS_SUB_4(Msg, method, topic, qos)                                                       \
    NROS_ASSERT_DECLARED_DEPTH(Msg::TYPE_NAME, (topic), (qos), #topic);                            \
    this->template create_subscription_in<                                                         \
        Msg, ::nros::detail::strip_ref<decltype(*this)>::type,                                     \
        &::nros::detail::strip_ref<decltype(*this)>::type::method>((topic), (qos))

/// Fewer than three arguments. Named rather than left undefined so the error is
/// about the CALL and not about an identifier the reader has never seen.
#define _NROS_SUB_TOO_FEW(...)                                                                     \
    static_assert(false, "NROS_SUBSCRIBE takes (Msg, method, topic) or "                           \
                         "(Msg, method, topic, qos) -- with the QoS omitted, the depth "           \
                         "declared in the contract sidecar supplies it.")

/// A subscription whose TOPIC is not a compile-time constant.
///
/// The compile-time check needs the topic as a constant expression to key the
/// table with; a topic built at runtime or forwarded through a variable has
/// none, so a call site like that names itself here and takes the BOOT-TIME
/// check in `Node::check_declared_depth` instead. That check is the same
/// comparison against the same table, and it halts boot naming the topic and
/// both depths.
///
/// This is the fallback and not the default on purpose: a build failure is
/// cheaper than a boot failure, and making the dynamic spelling explicit keeps
/// the count of call sites that gave up the compile-time check visible.
#define NROS_SUBSCRIBE_DYNAMIC(Msg, method, topic, qos)                                            \
    this->template create_subscription_in<                                                         \
        Msg, ::nros::detail::strip_ref<decltype(*this)>::type,                                     \
        &::nros::detail::strip_ref<decltype(*this)>::type::method>((topic), (qos))

/// Inside a `nros::NodeWithTimers<N>` constructor: create a repeating timer
/// calling `void Self::method()` every `period_ms`. Derives `Self` from `this`.
///
/// The verb is `create_wall_timer_in` (phase-427 W4): the pool-parked form takes
/// no storage argument, which would otherwise differ from upstream's
/// `create_wall_timer(duration, callback)` by SIGNATURE alone.
#define NROS_CREATE_WALL_TIMER(period_ms, method)                                                  \
    this->template create_wall_timer_in<                                                           \
        ::nros::detail::strip_ref<decltype(*this)>::type,                                          \
        &::nros::detail::strip_ref<decltype(*this)>::type::method>((period_ms))

// -- NROS_COMPONENT(Class) ---------------------------------------------------
//
// Marks a `nros::Node`-derived class as the pkg's rclcpp-faithful (IS-A-node)
// component. Parallels `NROS_NODE_REGISTER` (node_pkg.hpp), but for the
// construct-with-handle ctor shape.
//
// Emits the **factory** (placement-new with the entry's executor node handle) +
// the qualified **class** string + a **shape:"rclcpp"** marker. There is NO
// `sizeof`/`alignof` metadata: the typed codegen entry `#include`s the component
// header, so `sizeof(Class)` / a `Storage<Class>` is a compile-time fact there —
// not a codegen input.
//
// `NROS_PKG_NAME` is the cmake-injected (pre-sanitised) pkg token, same source
// as `NROS_NODE_REGISTER`. Hand-written pkgs `#define NROS_PKG_NAME my_pkg`
// before including this header.

#ifndef NROS_PKG_NAME
#define NROS_PKG_NAME unknown
#endif

#define _NROS_COMP_CAT_(a, b) a##b
#define _NROS_COMP_CAT(a, b) _NROS_COMP_CAT_(a, b)
#define _NROS_COMP_STR_(x) #x
#define _NROS_COMP_STR(x) _NROS_COMP_STR_(x)

#define _NROS_COMP_FACTORY_SYM(pkg) _NROS_COMP_CAT(__nros_component_factory_, pkg)
#define _NROS_COMP_CLASS_SYM(pkg) _NROS_COMP_CAT(__nros_component_class_, pkg)
#define _NROS_COMP_SHAPE_SYM(pkg) _NROS_COMP_CAT(__nros_component_shape_, pkg)

/// Register a `nros::Node`-derived class as the pkg's component. Emits:
///  - `__nros_component_factory_<pkg>(void* storage, void* node_handle)` — a
///    C-ABI factory that placement-news `Class(nros::NodeHandle(node_handle))`
///    into the entry-owned arena slot and returns it as `nros::Node*`.
///  - `__nros_component_class_<pkg>` — the `"<pkg>::<Class>"` string for lint.
///  - `__nros_component_shape_<pkg>` — the `"rclcpp"` shape marker.
///
/// The derived class MUST have an `explicit Class(nros::NodeHandle)` ctor (it
/// forwards the handle + the node name to the `Node` base).
///
/// The factory returns `::nros::Node*` — phase-427 W4. It used to return
/// `::nros::ComponentNode*`, a type that WRAPPED a node; the merged type IS one,
/// so the entry's post-construct `ok()` check now reads the node itself.
#define NROS_COMPONENT(Class)                                                                      \
    extern "C" ::nros::Node* _NROS_COMP_FACTORY_SYM(NROS_PKG_NAME)(void* storage,                  \
                                                                   void* node_handle) {            \
        return new (storage) Class(::nros::NodeHandle(node_handle));                               \
    }                                                                                              \
    extern "C" const char _NROS_COMP_CLASS_SYM(NROS_PKG_NAME)[] =                                  \
        _NROS_COMP_STR(NROS_PKG_NAME) "::" _NROS_COMP_STR(Class);                                  \
    extern "C" const char _NROS_COMP_SHAPE_SYM(NROS_PKG_NAME)[] = "rclcpp"

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

/// Convenience: bind a component timer member. Expands to the MEMBER overload
/// (phase-427 W3) — the retired `nros::bind_timer` would warn here, and a
/// deprecation a macro hides is not one.
#define NROS_BIND_TIMER(node, Class, method, out, period_ms, self)                                 \
    (node).template create_wall_timer<Class, &Class::method>((out), (period_ms), (self))

#endif // NROS_COMPONENT_HPP
