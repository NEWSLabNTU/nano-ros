// nros-cpp: the DISPATCH service client
// Freestanding C++ -- no exceptions, no STL required

/**
 * @file client.hpp
 * @ingroup grp_service
 * @brief `rclcpp::Client<S>` — the arena-registered (callback-style) service
 *        client, and `Client<S>::SharedPtr` = `nros::ClientHandle<S>`.
 *
 * The FUTURE-style client — caller-owned storage, `send_request()` / `call()` /
 * `wait_for_service()` — is `nros::PollClient<S>` in
 * `nros/polling_client.hpp` since phase-456 W9. See there for why the two are
 * separate types.
 */

#ifndef NROS_CPP_CLIENT_HPP
#define NROS_CPP_CLIENT_HPP

#include <cstdint>
#include <cstddef>

#include "nros/callback_context.hpp" // phase-456 W3 — the handler IS the arena context
#include "nros/client_handle.hpp"    // phase-456 W9 — what `Client<S>::SharedPtr` IS
#include "nros/config.hpp"
#include "nros/entity_name.hpp" // phase-444 — the one entity-name copy
#include "nros/result.hpp"
#include "nros/size_bound.hpp" // nros::detail::buffer_bounds<M>::tx — the request scratch bound

#include "nros_cpp_ffi.h"

// issue 1437 — `get_actual_qos()` returns a `nros::QoS` BY VALUE from an
// inline body, so the complete type must be here, not only by the time
// `nros/node.hpp` is pulled in below.
//
// AFTER `nros_cpp_ffi.h`, never before: `qos.hpp` defines the four
// `nros_cpp_qos_*_t` enums ITSELF under `#ifndef NROS_CPP_FFI_H`, so
// reaching it first makes the cbindgen header a REDEFINITION of all four.
#include "nros/qos.hpp"

// Phase 189.M3.3.f — `nros_cpp_service_client_register` is excluded from
// cbindgen (its Rust signature uses `RawResponseCallback`, an external-crate
// type alias). Declare it locally with a matching fn-ptr typedef.
// (`nros_cpp_service_client_send_on_handle` takes no callback, so it comes from
// the cbindgen header.)
extern "C" {
typedef void (*nros_cpp_service_response_callback_t)(const uint8_t* data, size_t len, void* ctx);

nros_cpp_ret_t nros_cpp_service_client_register(const nros_cpp_node_t* node,
                                                const char* service_name, const char* type_name,
                                                const char* type_hash, nros_cpp_qos_t qos,
                                                nros_cpp_service_response_callback_t callback,
                                                void* context, uint8_t sched_context,
                                                size_t* out_handle_id);
} // extern "C"

/// `rclcpp::Node` is named by the friend declaration below and by the
/// out-of-line `Node::create_*` bodies further down. `nros/node.hpp` (included
/// below, after the class, so a consumer pays only for the entities it uses)
/// has the definition; a qualified friend needs the name to EXIST first, which
/// an unqualified `friend class Node;` used to supply implicitly.
///
/// phase-427 W7 — declared in `rclcpp::`, which is where the definition moved.
/// An elaborated `class Node;` in `nros::` would now declare a SECOND, distinct
/// class and collide with the `rclcpp::Node` alias.
namespace rclcpp {
class Node;
}

// ============================================================================
// `rclcpp::Client<S>` -- DEFINED here (RFC-0089: rclcpp:: is the home)
// ============================================================================
//
// phase-428: the definition moved from `nros::` to `rclcpp::` and the alias
// turned around. The nested `SharedPtr` / `ConstSharedPtr` / `UniquePtr`
// aliases live on the class itself, so the rclcpp way of indexing types
// (`rclcpp::Client<S>::SharedPtr`) resolves with no wrapper in between.
namespace rclcpp {

/// Dispatch service client for a ROS 2 service — the rclcpp model.
///
/// A response handler is registered into the executor arena, which owns the
/// `RmwServiceClient` and runs the handler during `spin_once`. The service type
/// `S` must provide nested `Request` and `Response` types with `TYPE_NAME`,
/// `TYPE_HASH`, `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and
/// `ffi_deserialize()`.
///
/// THIS OBJECT IS BOOKKEEPING PLUS ONE VERB — phase-456 W9. The client, the
/// reply buffer, the handler and its context are all the arena's; W3 made the
/// arena's trampoline context the user's HANDLER rather than `&out`, so after
/// registration nothing of the caller's is referenced and this object is freely
/// movable.
///
/// What it does hold is what a REGISTRATION can be asked about and the arena
/// cannot be asked for by an index alone: `{initialized_, handle_id_,
/// executor_, service_name_}`. `executor_` and `handle_id_` are the pair that
/// names the arena entry (issue 1437) — and they are also the argument list of
/// `nros_cpp_service_client_send_on_handle`, which is why @ref
/// async_send_request needs nothing more. `service_name_` is the phase-444
/// C++-side copy, because the runtime takes the name at create and drops it.
///
/// Measured (phase-456 W3, re-measured W9), across `examples/`, `tests/`,
/// `book/` and `packages/`: a dispatch client has exactly ONE verb invoked on
/// it, `async_send_request`, at one example site plus the move probe W3 added.
/// That is why `Client<S>::SharedPtr` is a two-word `nros::ClientHandle<S>`
/// carrying that verb — not the empty keep-alive `ServiceHandle<S>` is, and not
/// an `Owned<Client<S>>`, because the arena owns the entity and the caller does
/// not.
///
/// Usage:
/// ```cpp
/// void on_response(const AddTwoInts::Response& resp) { /* ... */ }
/// rclcpp::Client<AddTwoInts> client;
/// NROS_TRY(node.create_client(client, "/add_two_ints", &on_response));
/// NROS_TRY(client.async_send_request(req));
/// // ... or, in ported shape:
/// auto handle = node.create_client<AddTwoInts>("/add_two_ints", &on_response);
/// NROS_TRY(handle.async_send_request(req));
/// ```
template <typename S> class Client {
  public:
    /// `rclcpp::Client<S>::SharedPtr` — phase-456 W9.
    ///
    /// `rclcpp::Client<S>::SharedPtr member_;` is how ported source declares a
    /// client member, so this alias must exist on every target — which
    /// `std::shared_ptr` does not.
    ///
    /// IT IS NOT A POINTER TO A `Client<S>`. A registered client is the
    /// arena's; what this names is `nros::ClientHandle<S>` — two words,
    /// copyable, carrying exactly the one verb the census says a dispatch client
    /// is asked for. See `client_handle.hpp`.
    using SharedPtr = ::nros::ClientHandle<S>;
    /// `rclcpp::Client<S>::ConstSharedPtr` — see `SharedPtr`. The same handle:
    /// `async_send_request` is `const` on it (the handle is two words the caller
    /// owns; the mutation is the arena's), so there is no mutable/const
    /// distinction to draw.
    using ConstSharedPtr = ::nros::ClientHandle<S>;
    /// `rclcpp::Client<S>::UniquePtr` — see `SharedPtr`.
    using UniquePtr = ::nros::ClientHandle<S>;

    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    /// Phase 189.M3.3.f — typed response-handler signature for the
    /// *callback-style* client (rclcpp async dispatch). The handler runs during
    /// `spin_once` when a reply arrives for a request sent via
    /// `async_send_request`.
    ///
    /// phase-456 W3 deleted the `TypedResponseFnWithCtx` sibling, for the
    /// reason stated on `Service<S>::TypedServiceFn`: the SFINAE guard on
    /// `Node::create_client` admits only a `void(*)(const Response&)`, so no
    /// overload could set it and the branch reading it was unreachable.
    using TypedResponseFn = void (*)(const ResponseType& response);

    /// Phase 189.M3.3.f — THE ONE VERB. Send a request; the reply is delivered
    /// to the registered response handler during `spin_once` (no Future).
    /// Returns immediately after sending.
    ///
    /// UPSTREAM PARITY: upstream's `async_send_request(req)` returns a
    /// `std::shared_future<Response::SharedPtr>` and its two-argument form takes
    /// a callback per REQUEST. Ours returns a `Result` and the handler is bound
    /// once, at registration — ledgered at `cpp:Client::async_send_request`,
    /// divergence / adopt. What changed in phase-456 W9 is only that the
    /// `callback_mode_` check is gone with the flag: every `rclcpp::Client<S>`
    /// is a dispatch client now, and the future-style road is
    /// `nros::PollClient<S>`.
    Result async_send_request(const RequestType& req) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t req_buf[::nros::detail::buffer_bounds<RequestType>::tx];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Result(::nros::ErrorCode::Error);
        }
        return Result(
            nros_cpp_service_client_send_on_handle(executor_, handle_id_, req_buf, req_len));
    }

    /// Check if the client was registered.
    bool is_valid() const { return initialized_; }

    /// Read back the service name this client was created on — phase-444.
    ///
    /// rclcpp's `ClientBase::get_service_name`, and the client half of "four
    /// entity families, one accessor" (ledger rows `cpp:Client::get_service_name`
    /// and `cpp:Service::get_service_name`): `Publisher` and `Subscription` have
    /// had `get_topic_name()` all along, both action tiers gained
    /// `get_action_name()` in phase-417 W4.b, and the service pair was the one
    /// family with no accessor in any C++ tier.
    ///
    /// The name lives C++-side in `service_name_`, for the reason
    /// `ActionServer::get_action_name` states: the runtime does NOT own it.
    /// `nros_cpp_service_client_register` takes `service_name` and drops it, so
    /// there is nothing FFI-side to hand back, and a borrowed `const char*`
    /// was rejected because `create_client` takes a pointer a hosted caller may
    /// well have obtained from a temporary.
    ///
    /// Returns `""` (never NULL) on an unregistered client, matching
    /// `Publisher::get_topic_name` and both action tiers.
    ///
    /// phase-456 W9 split this class in two and the method answers on BOTH
    /// halves, unchanged — the same shape `Service::get_service_name` took under
    /// W5. Each half keeps its own `service_name_`, because the copy is made at
    /// create from the argument the caller passed and neither owner can recover
    /// it afterwards.
    const char* get_service_name() const { return initialized_ ? service_name_ : ""; }

    /// The QoS the backend GRANTED this client's REQUEST endpoint — the
    /// publisher that sends calls. Issue 1437.
    ///
    /// `rclcpp::Client::get_request_publisher_actual_qos`. ONE
    /// `create_client` builds TWO endpoints that negotiate against DIFFERENT
    /// peers, so this and @ref get_response_subscription_actual_qos are two
    /// answers and neither stands for the other.
    ///
    /// A policy the backend cannot report is an ABSENCE (`ReliabilityUnknown`
    /// and friends), never the request echoed back — see
    /// @ref Publisher::get_actual_qos.
    ///
    /// phase-456 W9 — BOTH HALVES ANSWER, and the client comes out the way the
    /// SERVICE did rather than the way the subscription did.
    /// `PollSubscription::get_actual_qos` had to be left off the dispatch
    /// `Subscription<M>`, because a dispatch subscription holds only
    /// `sched_handle_id_` and reaching the arena would have meant inventing a
    /// signature that takes an executor (ledgered at
    /// `cpp:Subscription::get_actual_qos`). A dispatch client does NOT have that
    /// problem: issue 1437 already gave it `executor_` beside `handle_id_`, and
    /// `nros_cpp_service_client_get_actual_qos` serves both roads — `storage`
    /// for the owner, `(executor, handle_id)` for the arena. So the upstream
    /// no-argument spelling is reachable here with no weakening, and no ledger
    /// row is owed.
    ::nros::QoS get_request_publisher_actual_qos() const { return actual_qos_half(true); }

    /// The QoS the backend GRANTED this client's RESPONSE endpoint — the
    /// subscription that receives replies. Issue 1437; see
    /// @ref get_request_publisher_actual_qos.
    ::nros::QoS get_response_subscription_actual_qos() const { return actual_qos_half(false); }

    /// Executor arena slot for the registration; `SIZE_MAX` until registered.
    size_t handle_id() const { return handle_id_; }

    /// Destructor — there is nothing to release.
    ///
    /// The executor arena owns the client, the reply buffer and the handler, and
    /// frees them when the executor drops. No unregister FFI exists, so this
    /// cannot remove the registration and does not pretend to: it clears this
    /// object's own bookkeeping. Until phase-456 W9 the same destructor also
    /// freed a FUTURE-style client, behind an `if (initialized_ &&
    /// !callback_mode_)`; that half moved to `nros::PollClient<S>`, where the
    /// condition is unconditional.
    ~Client() { initialized_ = false; }

    // Move semantics (non-copyable). Bookkeeping only — there is no storage to
    // relocate, so `nros_cpp_service_client_relocate` is not called here.
    //
    // phase-456 W3 — a callback-style client is MOVABLE, and the warning that
    // used to stand here is gone with its subject. It said the arena holds
    // `this` as the response trampoline context; the arena holds the user's
    // HANDLER now, so nothing of the caller's is referenced after registration
    // and there is no pointer a move could leave stale. What survives the move
    // is `{executor_, handle_id_}`, which is what `async_send_request` needs and
    // all it needs.
    Client(Client&& other)
        : executor_(other.executor_), initialized_(other.initialized_),
          handle_id_(other.handle_id_), service_name_{} {
        ::nros::detail::assign_entity_name(service_name_, other.service_name_);
        other.initialized_ = false;
    }

    Client& operator=(Client&& other) {
        if (this != &other) {
            executor_ = other.executor_;
            initialized_ = other.initialized_;
            handle_id_ = other.handle_id_;
            ::nros::detail::assign_entity_name(service_name_, other.service_name_);
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor -- creates an unregistered service client.
    /// Use `Node::create_client()` to register one.
    Client() : executor_(nullptr), initialized_(false), service_name_{} {}

  private:
    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    friend class ::rclcpp::Node;

    /// Phase 189.M3.3.f — raw response trampoline matching `RawResponseCallback`
    /// (`void(data, len, ctx)`). Deserializes the reply, runs the user's typed
    /// handler.
    ///
    /// phase-456 W3 — `ctx` is the USER'S HANDLER, carried by value in the
    /// arena's context slot, not the `Client` object (`this`).
    static void response_trampoline(const uint8_t* data, size_t len, void* ctx) {
        const TypedResponseFn user_fn = ::nros::detail::fn_from_context<TypedResponseFn>(ctx);
        if (user_fn == nullptr) return;
        ResponseType response;
        if (ResponseType::ffi_deserialize(data, len, &response) != 0) return;
        user_fn(response);
    }

    /// The two directions differ only in which out-pointer is read, so one
    /// body serves both and they cannot end up swapped (issue 1437).
    ///
    /// `storage` is always NULL here: a dispatch client owns no
    /// `RmwServiceClient`, so `(executor_, handle_id_)` is the only road. The
    /// `callback_mode_ ? nullptr : storage_` branch this replaced existed
    /// because one class served two owners; `nros::PollClient<S>` takes the
    /// other arm and passes its `storage_` unconditionally.
    ::nros::QoS actual_qos_half(bool request) const {
        nros_cpp_qos_t req{};
        nros_cpp_qos_t resp{};
        if (!initialized_) return ::nros::detail::qos_all_unknown();
        if (nros_cpp_service_client_get_actual_qos(nullptr, executor_, handle_id_, &req, &resp) !=
            0) {
            return ::nros::detail::qos_all_unknown();
        }
        return ::nros::detail::qos_from_ffi(request ? req : resp);
    }

    // Callback-style BOOKKEEPING (Phase 189.M3.3.f). The handler itself is not
    // here — it lives in the arena (phase-456 W3) — and neither is the client,
    // so what remains is the caller's own record of a registration that refers
    // back to nothing of the caller's. `{executor_, handle_id_}` is also the
    // send road, which is why this object has a verb where `Service<S>` has
    // none.
    void* executor_;
    bool initialized_;
    size_t handle_id_ = static_cast<size_t>(-1);
    /// phase-444 — the service name, kept C++-side for `get_service_name()`.
    /// `::nros::SERVICE_NAME_MAX` bytes, the same bound every other entity
    /// family uses.
    char service_name_[::nros::SERVICE_NAME_MAX];
};

} // namespace rclcpp

// ============================================================================
// nros:: -- the in-tree spelling, now the ALIAS (RFC-0089). Declared here,
// before the out-of-line `Node::create_*` bodies below, which are written in
// the `nros::` vocabulary.
// ============================================================================
namespace nros {
template <typename S> using Client = ::rclcpp::Client<S>;
} // namespace nros

// Phase 84.G8: out-of-line definition of Node::create_client<S>().
#include "nros/node.hpp"

namespace nros {

// Phase 189.M3.3.f — callback-style (arena-registered) client. The arena owns
// the client AND the response handler, and dispatches it during spin_once;
// requests go through `async_send_request`, which needs only `out`'s own
// `{executor_, handle_id_}`. `options.sched_context` is functional.
} // namespace nros

namespace rclcpp {
template <typename S, typename F, typename>
Result Node::create_client(Client<S>& out, const char* service_name, F callback,
                           const ::nros::QoS& qos, const ::nros::ClientOptions& options) {
    if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::nros::detail::qos_to_ffi(qos);

    // The user handler becomes the ARENA's context (phase-456 W3); the
    // conversion is still the compile error for a non-convertible `F`.
    const typename Client<S>::TypedResponseFn user_fn =
        typename Client<S>::TypedResponseFn(callback);

    uint8_t sched = (options.sched_context == ::nros::SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    nros_cpp_ret_t ret = nros_cpp_service_client_register(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos,
        reinterpret_cast<nros_cpp_service_response_callback_t>(&Client<S>::response_trampoline),
        ::nros::detail::fn_to_context(user_fn), sched, &handle);
    if (ret == 0) {
        out.executor_ = executor_handle_;
        out.handle_id_ = handle;
        // phase-444 — see `nros::PollClient`'s overload, which does the same.
        ::nros::detail::assign_entity_name(out.service_name_, service_name);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace nros {} // namespace nros

#endif // NROS_CPP_CLIENT_HPP
