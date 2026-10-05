// nros-cpp: the dispatch service client's handle
// Freestanding C++ — no exceptions, no STL required

/**
 * @file client_handle.hpp
 * @ingroup grp_service
 * @brief `nros::ClientHandle<S>` — what `Client<S>::SharedPtr` is: a handle
 *        over an arena entry, carrying the verbs `{executor, handle_id}`
 *        answers.
 */

#ifndef NROS_CPP_CLIENT_HANDLE_HPP
#define NROS_CPP_CLIENT_HANDLE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/log.hpp" // phase-417 stage 3 — NROS_RCLCPP_REFUSE_* + rclcpp::detail::refuse
#include "nros/result.hpp"
#include "nros/size_bound.hpp" // nros::detail::buffer_bounds<M>::tx — the request scratch bound

#include "nros_cpp_ffi.h"

/// `rclcpp::Client<S>` is what this handle REFERS to, and `element_type` below
/// names it. Declared rather than included: `nros/client.hpp` includes THIS
/// header (the nested `SharedPtr` alias needs the type), and an alias member is
/// satisfied by an incomplete type.
namespace rclcpp {
template <typename S> class Client;
}

namespace nros {

/// A registered dispatch service client — phase-456 W9.
///
/// WHY THIS ONE IS NOT `ServiceHandle`'s TWIN, AND THE DIFFERENCE IS THAT IT
/// HAS VERBS AT ALL
///
/// `SubscriptionHandle<M>` and `ServiceHandle<S>` are two words with no method,
/// because phase-456 W2 and W3 measured that the ported corpus invokes NOTHING
/// on a dispatch subscription or a dispatch service: each is stored and dropped,
/// which is what a keep-alive is. W3 ran the same census over clients and got a
/// DIFFERENT answer — `async_send_request`, at one site, and nothing else. So a
/// bare keep-alive would be too little here and `Owned<Client<S>>` too much (the
/// arena owns the entity, not the caller). This is the two-word handle with that
/// verb on it.
///
/// Re-measured on this base (phase-456 W9, the same four trees: `examples/`,
/// `tests/`, `book/`, `packages/`): still ONE verb INVOKED, still one site —
/// `examples/native/cpp/service-client-callback/src/main.cpp:95`, plus
/// `tests/compile/bind_service.cpp:139`, which W3 added to pin that a MOVED
/// registered client can still send. Nothing else is invoked on a dispatch
/// client anywhere.
///
/// 2026-09-28 added the two DISCOVERY reads — @ref service_is_ready and @ref
/// wait_for_service — and nothing about that census changed: no in-tree site
/// calls them on a handle yet. They are here because a ported node reaches for
/// them (upstream's own service tutorial opens with `wait_for_service`), and
/// because they now cost nothing to offer: both FFI entry points take
/// `(executor, handle_id)`, so the handle stays TWO WORDS and gains no member.
///
/// WHAT IT IS, AND WHY IT IS NOT A POINTER TO A `Client<S>`
///
/// A client created with a RESPONSE HANDLER is owned by the Rust executor
/// arena: `nros_cpp_service_client_register` returns a handle id, the arena
/// holds the `RmwServiceClient`, and since phase-456 W3 it holds the user's
/// handler as its own trampoline context. Nothing of the caller's is referenced
/// after registration, so there is no C++ object to point at and none is made.
///
/// `{executor, handle_id}` is not bookkeeping here — it is the ARGUMENT LIST of
/// `nros_cpp_service_client_send_on_handle`. That is why the verb fits on two
/// words: the send road for a dispatch client never goes through caller storage.
///
/// WHAT IT DELIBERATELY DOES NOT HAVE
///
/// No `operator->`. There is no object at the other end, and the verb is a
/// member of the handle itself, so a ported `client_->async_send_request(req)`
/// is a compile error naming `ClientHandle` and the mechanical edit is one
/// character (`->` becomes `.`). A self-returning `operator->` would have made
/// that line compile by claiming a pointee that does not exist, and it would
/// have made `client_->send_request(...)` fail with a message about the wrong
/// type. Measured: no in-tree site writes `->` on a dispatch client handle, so
/// nothing pays for this today.
///
/// No `send_request` / `call` / `call_polling`, nor their `_sized` forms. Those
/// read an `RmwServiceClient` AND a reply buffer in CALLER storage, and a
/// dispatch client has neither — that road is `nros::PollClient<S>`
/// (`nros/polling_client.hpp`). Offering them here is the defect the W2b/W5
/// splits removed one entity at a time.
///
/// `wait_for_service` and `service_is_ready` WERE on that list, and 2026-09-28
/// took them off it — the line to draw is what a verb READS, not which half
/// declared it. Both ask the backend about DISCOVERY and touch nothing of the
/// caller's, so once their FFI entry points gained the `(executor, handle_id)`
/// arm (the shape `nros_cpp_service_client_get_actual_qos` has had since issue
/// 1437) they need exactly the two words this handle already carries. Until then
/// a porter holding a `Client<S>::SharedPtr` could not wait for a server at all,
/// which is the gap W9 ledgered at `cpp:Client::wait_for_service` rather than
/// papering over with an invented `Executor&` parameter.
///
/// HOW IT ENDS (issue 1667)
///
/// `cli_.reset()` RELEASES the client, as `timer_.reset()` releases a timer:
/// the client leaves the graph, a reply it was still waiting for is abandoned,
/// and the arena entry and the response handler's capture are freed for the
/// next registration. Destroying the node that created it does the same.
/// Dropping or overwriting a handle does NOT. A stale copy carries the
/// generation of the registration it was issued for (phase-476 W0), so after a
/// reset it answers `Error` and never reaches the slot's next occupant.
template <typename S> class ClientHandle {
  public:
    /// The service type, for the same reason `std::shared_ptr` exposes one.
    using service_type = S;
    /// What this handle refers to. A NAME, not a dereference — there is no
    /// `operator->`, and no C++ object at the other end.
    using element_type = ::rclcpp::Client<S>;

    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    constexpr ClientHandle() : executor_(nullptr), handle_id_(0) {}
    /// Null, spelled the way a ported file spells it
    /// (`Client<S>::SharedPtr cli_ = nullptr;`).
    constexpr ClientHandle(decltype(nullptr)) : executor_(nullptr), handle_id_(0) {}
    constexpr ClientHandle(void* executor, size_t handle_id)
        : executor_(executor), handle_id_(handle_id) {}

    /// `if (cli_)` / `if (!cli_)`. A default-constructed handle is empty; one
    /// from a successful `create_client` never is.
    explicit constexpr operator bool() const { return executor_ != nullptr; }

    /// THE SEND VERB — send a request; the reply reaches the handler the
    /// registration carries, during `spin_once`.
    ///
    /// Same body as @ref rclcpp::Client::async_send_request, on the same two
    /// words, because that method needed no more than these two. See that one
    /// for what upstream's return differs by (a future, which our dispatch road
    /// does not produce — the reply goes to the handler).
    ///
    /// Returns `NotInitialized` on an empty handle, `Error` if the request does
    /// not serialize, and the transport's code otherwise.
    Result async_send_request(const RequestType& req) const {
        if (executor_ == nullptr) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t req_buf[::nros::detail::buffer_bounds<RequestType>::tx];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Result(::nros::ErrorCode::Error);
        }
        return Result(
            nros_cpp_service_client_send_on_handle(executor_, handle_id_, req_buf, req_len));
    }

    /// Is a matching service server visible? — `rclcpp::ClientBase::
    /// service_is_ready`, no arguments, as upstream's is. Same body and same
    /// tri-state as @ref rclcpp::Client::service_is_ready, on the same two
    /// words; see that method for what the three answers mean and why the
    /// result is not a bare `bool`.
    ///
    /// `NotInitialized` on an empty handle.
    ::nros::ResultOf<bool> service_is_ready() const {
        if (executor_ == nullptr) {
            return ::nros::ResultOf<bool>::error(::nros::ErrorCode::NotInitialized);
        }
        int out = -1;
        nros_cpp_ret_t ret =
            nros_cpp_service_client_server_available(nullptr, executor_, handle_id_, &out);
        if (ret != 0) return ::nros::ResultOf<bool>::error(static_cast<::nros::ErrorCode>(ret));
        if (out < 0) return ::nros::ResultOf<bool>::error(::nros::ErrorCode::Unsupported);
        return ::nros::ResultOf<bool>::ok(out != 0);
    }

    /// Block until a matching service server is discoverable —
    /// `rclcpp::ClientBase::wait_for_service`. Spins the executor cooperatively,
    /// so not from inside a callback; use @ref service_is_ready there.
    ///
    /// The budget is REQUIRED, and the no-argument form below says why.
    Result wait_for_service(uint32_t timeout_ms) const {
        if (executor_ == nullptr) return Result(::nros::ErrorCode::NotInitialized);
        return Result(
            nros_cpp_service_client_wait_for_service(nullptr, executor_, handle_id_, timeout_ms));
    }

    /// **REFUSED** — `wait_for_service()` with no budget. phase-417 stage 3,
    /// reaching this type 2026-09-28 with the verb itself: the refusal is the
    /// other half of adopting the name, since a ported argument-free call is
    /// exactly what upstream's tutorials write.
    template <typename T = void> Result wait_for_service() const {
        static_assert(::rclcpp::detail::refuse<T>::value, NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT);
        return Result(::nros::ErrorCode::Unsupported);
    }

    /// The executor arena slot this registration occupies. Present for
    /// introspection and for tests that assert a registration happened; it is
    /// also half of what @ref async_send_request sends on.
    constexpr size_t handle_id() const { return handle_id_; }

    /// `cli_.reset()` — release the client (issue 1667): it leaves the graph,
    /// a pending reply is abandoned, and this handle becomes empty. After it,
    /// @ref async_send_request answers `NotInitialized` on this handle and
    /// `Error` on any other copy. A no-op on an empty handle.
    void reset() {
        if (executor_ != nullptr) {
            (void)nros_cpp_service_client_release(executor_, handle_id_);
        }
        executor_ = nullptr;
        handle_id_ = 0;
    }

  private:
    void* executor_;
    size_t handle_id_;
};

template <typename S>
constexpr bool operator==(const ClientHandle<S>& a, const ClientHandle<S>& b) {
    return a.handle_id() == b.handle_id() && static_cast<bool>(a) == static_cast<bool>(b);
}
template <typename S>
constexpr bool operator!=(const ClientHandle<S>& a, const ClientHandle<S>& b) {
    return !(a == b);
}
template <typename S> constexpr bool operator==(const ClientHandle<S>& a, decltype(nullptr)) {
    return !static_cast<bool>(a);
}
template <typename S> constexpr bool operator!=(const ClientHandle<S>& a, decltype(nullptr)) {
    return static_cast<bool>(a);
}
template <typename S> constexpr bool operator==(decltype(nullptr), const ClientHandle<S>& a) {
    return !static_cast<bool>(a);
}
template <typename S> constexpr bool operator!=(decltype(nullptr), const ClientHandle<S>& a) {
    return static_cast<bool>(a);
}

} // namespace nros

#endif // NROS_CPP_CLIENT_HANDLE_HPP
