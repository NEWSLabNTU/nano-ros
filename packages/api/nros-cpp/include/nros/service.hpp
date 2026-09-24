// nros-cpp: the DISPATCH service server
// Freestanding C++ — no exceptions, no STL required

/**
 * @file service.hpp
 * @ingroup grp_service
 * @brief `rclcpp::Service<S>` — the arena-registered (callback-style) service
 *        server, and `Service<S>::SharedPtr` = `nros::ServiceHandle<S>`.
 *
 * The POLL-style server — caller-owned storage, `take_request()` /
 * `send_response()` — is `nros::PollService<S>` in
 * `nros/polling_service.hpp` since phase-456 W5. See there for why the two
 * are separate types.
 */

#ifndef NROS_CPP_SERVICE_HPP
#define NROS_CPP_SERVICE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/callback_context.hpp" // phase-456 W3 — the handler IS the arena context
#include "nros/config.hpp"
#include "nros/entity_name.hpp" // phase-444 — the one entity-name copy
#include "nros/result.hpp"
#include "nros/service_handle.hpp" // phase-456 W5 — what `Service<S>::SharedPtr` IS
#include "nros/size_bound.hpp"     // nros::rx_buffer_capacity<M> — the receive-buffer size

#include "nros_cpp_ffi.h"

// issue 1437 — the two granted-QoS accessors return a `nros::QoS` BY VALUE from
// an inline body, so the complete type must be here, not only by the time
// `nros/node.hpp` is pulled in below.
//
// AFTER `nros_cpp_ffi.h`, never before: `qos.hpp` defines the four
// `nros_cpp_qos_*_t` enums ITSELF under `#ifndef NROS_CPP_FFI_H`, so
// reaching it first makes the cbindgen header a REDEFINITION of all four.
#include "nros/qos.hpp"

// Phase 189.M3.3.e — `nros_cpp_service_server_register` is excluded from
// cbindgen (its Rust signature uses `RawServiceCallback`, an external-crate
// type alias cbindgen names without defining). Declare it locally with a plain
// function-pointer typedef matching the ABI (`bool(req, req_len, resp,
// resp_cap, resp_len, ctx)`).
extern "C" {
typedef bool (*nros_cpp_service_request_callback_t)(const uint8_t* req, size_t req_len,
                                                    uint8_t* resp, size_t resp_cap,
                                                    size_t* resp_len, void* ctx);

nros_cpp_ret_t nros_cpp_service_server_register(const nros_cpp_node_t* node,
                                                const char* service_name, const char* type_name,
                                                const char* type_hash, nros_cpp_qos_t qos,
                                                nros_cpp_service_request_callback_t callback,
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
// `rclcpp::Service<S>` -- DEFINED here (RFC-0089: rclcpp:: is the home)
// ============================================================================
//
// phase-428: the definition moved from `nros::` to `rclcpp::` and the alias
// turned around. The nested `SharedPtr` / `ConstSharedPtr` / `UniquePtr`
// aliases live on the class itself, so the rclcpp way of indexing types
// (`rclcpp::Service<S>::SharedPtr`) resolves with no wrapper in between.
namespace rclcpp {

/// Dispatch service server for a ROS 2 service — the rclcpp model.
///
/// A handler is registered into the executor arena, which owns the
/// `RmwServiceServer` and runs the handler during `spin_once`. The service type
/// `S` must provide nested `Request` and `Response` types with `TYPE_NAME`,
/// `TYPE_HASH`, `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and
/// `ffi_deserialize()`.
///
/// THIS OBJECT IS BOOKKEEPING — phase-456 W5. The server, the request buffer,
/// the handler and its context are all the arena's; W3 made the arena's
/// trampoline context the user's HANDLER rather than `&out`, so after
/// registration nothing of the caller's is referenced and this object is freely
/// movable.
///
/// What it does hold is what a REGISTRATION can be asked about and the arena
/// cannot be asked for by an index alone: `{initialized_, handle_id_,
/// executor_, service_name_}`. `executor_` and `handle_id_` are the pair that
/// names the arena entry (issue 1437) — without the executor an index points
/// into an arena this object cannot reach — and `service_name_` is the
/// phase-444 C++-side copy, because the runtime takes the name at create and
/// drops it. Neither survives as state the arena refers back to; both are the
/// caller's own record.
///
/// Measured (phase-456 W3), across `examples/`, `tests/`, `book/` and
/// `packages/`: a dispatch service has **nothing invoked on it**, at any site,
/// out-ref or `::SharedPtr`. It is a keep-alive — which is why
/// `Service<S>::SharedPtr` is a two-word `nros::ServiceHandle<S>` with no
/// `operator->` rather than a pointer to one of these.
///
/// Usage:
/// ```cpp
/// void add(const AddTwoInts::Request& req, AddTwoInts::Response& resp) {
///     resp.sum = req.a + req.b;
/// }
/// rclcpp::Service<AddTwoInts> srv;
/// NROS_TRY(node.create_service(srv, "/add_two_ints", &add));
/// // ... or, in ported shape:
/// auto handle = node.create_service<AddTwoInts>("/add_two_ints", &add);
/// ```
template <typename S> class Service {
  public:
    /// `rclcpp::Service<S>::SharedPtr` — phase-456 W5.
    ///
    /// `rclcpp::Service<S>::SharedPtr member_;` is how ported source declares a
    /// service member, so this alias must exist on every target — which
    /// `std::shared_ptr` does not.
    ///
    /// IT IS NOT A POINTER TO A `Service<S>`. A registered service is the
    /// arena's; what this names is `nros::ServiceHandle<S>` — two words,
    /// copyable, carrying only what a registration can perform, which the
    /// census says is nothing but "exist". See `service_handle.hpp`.
    using SharedPtr = ::nros::ServiceHandle<S>;
    /// `rclcpp::Service<S>::ConstSharedPtr` — see `SharedPtr`. The same handle:
    /// there is no mutable/const distinction to draw over a registration that
    /// exposes no operation on the entity.
    using ConstSharedPtr = ::nros::ServiceHandle<S>;
    /// `rclcpp::Service<S>::UniquePtr` — see `SharedPtr`.
    using UniquePtr = ::nros::ServiceHandle<S>;

    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    /// Phase 189.M3.3.e — typed request-handler signature for the
    /// *callback-style* service (rclcpp dispatch model). The handler fills
    /// `response` from `request`; the executor sends the reply during spin.
    ///
    /// phase-456 W3 deleted the `TypedServiceFnWithCtx` sibling. It was
    /// write-only state: the SFINAE guard on `Node::create_service` admits only
    /// a `void(*)(const Request&, Response&)`, so no overload could ever set it,
    /// and the `else if` branch that read it in the trampoline was unreachable.
    /// A handler that wants context binds it at compile time instead —
    /// `nros::bind_service<Svc, C, &C::method>` in `component.hpp`, where `this` is
    /// the context and no runtime pointer pair is needed.
    using TypedServiceFn = void (*)(const RequestType& request, ResponseType& response);

    /// Check if the service was registered.
    bool is_valid() const { return initialized_; }

    /// Read back the service name this server was registered on — phase-444.
    ///
    /// rclcpp's `ServiceBase::get_service_name`, and the server half of
    /// `Client::get_service_name`; see that method for why the name lives
    /// C++-side and why the accessor returns `""` rather than NULL on an
    /// unregistered server.
    ///
    /// phase-456 W5 split this class in two and the method answers on BOTH
    /// halves, unchanged — the same shape `PollSubscription::get_publisher_count`
    /// took under W2b. It reads `service_name_`, which each half keeps its own
    /// copy of, because the copy is made at create from the argument the caller
    /// passed and neither owner can recover it afterwards: the runtime takes the
    /// name and drops it, and there is no FFI that reads a name back out of the
    /// arena. A shared copy would have to live somewhere neither half owns.
    const char* get_service_name() const { return initialized_ ? service_name_ : ""; }

    /// The QoS the backend GRANTED this service's REQUEST endpoint — the
    /// subscription that receives calls. Issue 1437.
    ///
    /// `rclcpp::Service::get_request_subscription_actual_qos`. ONE
    /// `create_service` builds TWO endpoints that negotiate against DIFFERENT
    /// peers, so this and @ref get_response_publisher_actual_qos are two
    /// answers and neither stands for the other: a reliable reply path over a
    /// best-effort request path is a working service that drops calls.
    ///
    /// A policy the backend cannot report is an ABSENCE (`ReliabilityUnknown`
    /// and friends), never the request echoed back — see
    /// @ref Publisher::get_actual_qos.
    ///
    /// phase-456 W5 — BOTH HALVES ANSWER, and the service comes out the other
    /// way from the subscription. `PollSubscription::get_actual_qos` had to be
    /// left off the dispatch half, because a dispatch `Subscription<M>` holds
    /// only `sched_handle_id_` and reaching the arena would have meant inventing
    /// a signature that takes an executor (ledgered at
    /// `cpp:Subscription::get_actual_qos`). A dispatch service does NOT have that
    /// problem: issue 1437 already gave it `executor_` beside `handle_id_`, and
    /// `nros_cpp_service_server_get_actual_qos` serves both roads — `storage_`
    /// for the owner, `(executor, handle_id)` for the arena. So the upstream
    /// no-argument spelling is reachable here with no weakening, and no ledger
    /// row is owed.
    ::nros::QoS get_request_subscription_actual_qos() const { return actual_qos_half(true); }

    /// The QoS the backend GRANTED this service's RESPONSE endpoint — the
    /// publisher that sends replies. Issue 1437; see
    /// @ref get_request_subscription_actual_qos.
    ::nros::QoS get_response_publisher_actual_qos() const { return actual_qos_half(false); }

    /// Destructor — there is nothing to release.
    ///
    /// The executor arena owns the server, the request buffer and the handler,
    /// and frees them when the executor drops. No unregister FFI exists, so
    /// this cannot remove the registration and does not pretend to: it clears
    /// this object's own bookkeeping. Until phase-456 W5 the same destructor
    /// also freed a POLL server, behind an `if (initialized_ &&
    /// !callback_mode_)`; that half moved to `nros::PollService<S>`, where the
    /// condition is unconditional.
    ~Service() { initialized_ = false; }

    // Move semantics (non-copyable). Bookkeeping only — there is no storage to
    // relocate, so `nros_cpp_service_server_relocate` is not called here.
    //
    // phase-456 W3 — a callback-style service is MOVABLE, and the warning that
    // used to stand here is gone with its subject. It said:
    //
    //     A callback-style service must NOT be moved after register — the arena
    //     holds `this` as the trampoline context (Phase 189.M3.3.e); the move
    //     only transfers bookkeeping and leaves that pointer stale, so don't.
    //
    // The arena holds the user's HANDLER as its context now, not `this`, so
    // after registration nothing of the caller's is referenced and there is no
    // pointer a move could leave stale. What moves is bookkeeping, which is
    // what the warning said it was — the difference is that bookkeeping is now
    // all there is.
    Service(Service&& other)
        : initialized_(other.initialized_), handle_id_(other.handle_id_), service_name_{},
          executor_(other.executor_) {
        ::nros::detail::assign_entity_name(service_name_, other.service_name_);
        other.initialized_ = false;
    }

    Service& operator=(Service&& other) {
        if (this != &other) {
            initialized_ = other.initialized_;
            handle_id_ = other.handle_id_;
            ::nros::detail::assign_entity_name(service_name_, other.service_name_);
            // issue 1437 — moves with the index it pairs with.
            executor_ = other.executor_;
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor — creates an unregistered service server.
    /// Use `Node::create_service()` to register one.
    Service() : initialized_(false), service_name_{} {}

    /// Executor arena slot for the registration; `SIZE_MAX` until registered.
    size_t handle_id() const { return handle_id_; }

  private:
    Service(const Service&) = delete;
    Service& operator=(const Service&) = delete;

    friend class ::rclcpp::Node;

    /// Phase 189.M3.3.e — raw request trampoline matching `RawServiceCallback`
    /// (`bool(req, req_len, resp, resp_cap, resp_len, ctx)`). Deserializes the
    /// request, runs the user's typed handler, serializes the response.
    ///
    /// phase-456 W3 — `ctx` is the USER'S HANDLER, carried by value in the
    /// arena's own context slot (`nros::detail::fn_to_context`). It used to be
    /// the `Service` object (`this`), which is what made the arena hold the
    /// address of a caller-side object and what the move constructor had to
    /// warn about. Nothing of the caller's is referenced here now.
    static bool request_trampoline(const uint8_t* req, size_t req_len, uint8_t* resp,
                                   size_t resp_cap, size_t* resp_len, void* ctx) {
        const TypedServiceFn user_fn = ::nros::detail::fn_from_context<TypedServiceFn>(ctx);
        if (user_fn == nullptr) return false;
        RequestType request;
        if (RequestType::ffi_deserialize(req, req_len, &request) != 0) return false;
        ResponseType response;
        user_fn(request, response);
        size_t len = 0;
        if (ResponseType::ffi_serialize(&response, resp, resp_cap, &len) != 0) return false;
        *resp_len = len;
        return true;
    }

    /// The two directions differ only in which out-pointer is read, so one
    /// body serves both and they cannot end up swapped (issue 1437).
    ///
    /// `storage` is always NULL here: a dispatch service owns no
    /// `RmwServiceServer`, so `(executor_, handle_id_)` is the only road. The
    /// `callback_mode_ ? nullptr : storage_` branch this replaced existed
    /// because one class served two owners; `nros::PollService<S>` takes the
    /// other arm and passes its `storage_` unconditionally.
    ::nros::QoS actual_qos_half(bool request) const {
        nros_cpp_qos_t req{};
        nros_cpp_qos_t resp{};
        if (!initialized_) return ::nros::detail::qos_all_unknown();
        if (nros_cpp_service_server_get_actual_qos(nullptr, executor_, handle_id_, &req, &resp) !=
            0) {
            return ::nros::detail::qos_all_unknown();
        }
        return ::nros::detail::qos_from_ffi(request ? req : resp);
    }

    bool initialized_;
    // Callback-style BOOKKEEPING (Phase 189.M3.3.e). The handler itself is not
    // here — it lives in the arena (phase-456 W3) — and neither is the server,
    // so what remains is the caller's own record of a registration that refers
    // back to nothing of the caller's.
    size_t handle_id_ = static_cast<size_t>(-1);

    /// phase-444 — the service name, kept C++-side for `get_service_name()`.
    /// See `Client`'s field of the same name.
    char service_name_[::nros::SERVICE_NAME_MAX];

    // issue 1437 — the executor whose arena holds this service, recorded with
    // the index that names it there. `handle_id_` alone points into an arena
    // this object could not otherwise reach.
    void* executor_ = nullptr;
};

} // namespace rclcpp

// ============================================================================
// nros:: -- the in-tree spelling, now the ALIAS (RFC-0089). Declared here,
// before the out-of-line `Node::create_*` bodies below, which are written in
// the `nros::` vocabulary.
// ============================================================================
namespace nros {
template <typename S> using Service = ::rclcpp::Service<S>;
} // namespace nros

// Phase 84.G8: out-of-line definition of Node::create_service<S>().
#include "nros/node.hpp"

namespace nros {

// Phase 189.M3.3.e — callback-style (arena-registered) service. The arena owns
// the server AND the request handler, and dispatches it during spin_once, so
// the handle is real and `options.sched_context` is functional. `out` receives
// the handle id and nothing the arena refers back to (phase-456 W3).
} // namespace nros

namespace rclcpp {
template <typename S, typename F, typename>
Result Node::create_service(Service<S>& out, const char* service_name, F callback,
                            const ::nros::QoS& qos, const ::nros::ServiceOptions& options) {
    if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::nros::detail::qos_to_ffi(qos);

    // The user handler becomes the ARENA's context (phase-456 W3). The
    // conversion is still the compile error for a non-convertible `F`; what
    // changed is where the resulting pointer is stored — in the registration,
    // not in `out`, so `out` is no longer an object the arena knows by address.
    const typename Service<S>::TypedServiceFn user_fn =
        typename Service<S>::TypedServiceFn(callback);

    uint8_t sched = (options.sched_context == ::nros::SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    nros_cpp_ret_t ret = nros_cpp_service_server_register(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos,
        reinterpret_cast<nros_cpp_service_request_callback_t>(&Service<S>::request_trampoline),
        ::nros::detail::fn_to_context(user_fn), sched, &handle);
    if (ret == 0) {
        out.handle_id_ = handle;
        // phase-444 — remember the name for `get_service_name()`; the runtime
        // takes `service_name` and drops it.
        ::nros::detail::assign_entity_name(out.service_name_, service_name);

        // issue 1437 — the arena that owns the entity, beside its index.
        out.executor_ = executor_handle_;
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace nros {} // namespace nros

#endif // NROS_CPP_SERVICE_HPP
