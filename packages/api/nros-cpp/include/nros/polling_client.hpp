// nros-cpp: the poll-style (future-style) service client
// Freestanding C++ -- no exceptions, no STL required

/**
 * @file polling_client.hpp
 * @ingroup grp_service
 * @brief `nros::PollClient<S>` — the service client the CALLER owns and drains.
 *
 * WHICH ONE DO I WANT
 *
 *   `PollClient<S>`      — the caller owns an `RmwServiceClient` in its own
 *                          storage, drives `spin_once()` itself, and drains the
 *                          reply with a `Future` (`send_request`), a blocking
 *                          `call()`, or `call_polling()`. This is the type that
 *                          carries every verb that reads caller storage,
 *                          `wait_for_service` and `service_is_ready` included.
 *   `rclcpp::Client<S>`  — the DISPATCH client (`nros/client.hpp`). A response
 *                          handler is registered into the executor arena, which
 *                          owns the client and runs the handler during spin. Its
 *                          one verb is `async_send_request`.
 *
 * WHY THE DRAINING API LIVES HERE — phase-456 W9, following W2b and W5
 *
 * It used to live on `rclcpp::Client<S>`, which served BOTH ownership models
 * behind a `callback_mode_` flag. On the dispatch path the arena owns the client
 * and `storage_` is never filled, so `wait_for_service()` on a callback-mode
 * object handed `NROS_SERVICE_CLIENT_SIZE` value-initialized zero bytes to
 * `nros_cpp_service_client_wait_for_service` — whose own doc comment says
 * `storage` must be "a valid initialized future-style service client". Nothing
 * on that path checked; `initialized_` is true for both. `service_is_ready()`,
 * `server_available()`, `send_request()`, `call()` and `call_polling()` were all
 * reachable the same way. The split is what makes those calls UNWRITABLE rather
 * than merely wrong, which is the same reason and the same remedy W2b applied to
 * `nros::PollSubscription<M>` and W5 to `nros::PollService<S>`.
 *
 * It is also what unblocks the alias. `Node::create_client<S>(name, qos)` with
 * no handler returns and exists to be `->send_request()`'d, so while one class
 * served both, `Client<S>::SharedPtr` had to be both a handle and a pointer to a
 * future-style object. W3 recorded that collision as the blocker and W6 costed
 * it; this is its removal.
 *
 * UPSTREAM PARITY, STATED: upstream has no poll-style service client — a
 * `rclcpp::Client` belongs to the node, and a reply is taken either through the
 * future `async_send_request` returns or through a callback given to it. So this
 * type is OURS, and the upstream NAMES it carries (`wait_for_service`,
 * `service_is_ready`, `get_service_name`, the two granted-QoS accessors) each
 * carry a ledger row of their own rather than inheriting `cpp:PollClient`'s
 * verdict silently — the shape W5 settled for `PollService`.
 */

#ifndef NROS_CPP_POLLING_CLIENT_HPP
#define NROS_CPP_POLLING_CLIENT_HPP

#include <cstdint>
#include <cstddef>

#include "nros/config.hpp"
#include "nros/entity_name.hpp" // phase-444 — the one entity-name copy
#include "nros/future.hpp"
#include "nros/log.hpp" // phase-417 stage 3 — NROS_RCLCPP_REFUSE_* + rclcpp::detail::refuse
#include "nros/result.hpp"
#include "nros/size_bound.hpp" // nros::rx_buffer_capacity<M> — the receive-buffer size

#include "nros_cpp_ffi.h"

// issue 1437 — the two granted-QoS accessors return a `nros::QoS` BY VALUE from
// an inline body, so the complete type must be here, not only by the time
// `nros/node.hpp` is pulled in below.
//
// AFTER `nros_cpp_ffi.h`, never before: `qos.hpp` defines the four
// `nros_cpp_qos_*_t` enums ITSELF under `#ifndef NROS_CPP_FFI_H`, so
// reaching it first makes the cbindgen header a REDEFINITION of all four.
#include "nros/qos.hpp"

// phase-427 W7 — `Node` is DEFINED in `rclcpp::`. The friend declaration below
// is qualified, and a qualified friend names an existing entity rather than
// introducing one, so the name has to be declared first — and in `rclcpp::`,
// because an elaborated `class Node;` in `nros::` would declare a second,
// distinct class.
namespace rclcpp {
class Node;
}

namespace nros {

/// Poll-style service client — caller-owned storage, caller-driven reply.
///
/// phase-456 W9: this is the class `rclcpp::Client<S>` used to be when it was
/// created through the out-ref `create_client(out, name, qos)`.
///
/// Usage (future — preferred):
/// ```cpp
/// nros::PollClient<example_interfaces::srv::AddTwoInts> client;
/// NROS_TRY(node.create_client(client, "/add_two_ints"));
/// NROS_TRY(client.wait_for_service(10000));
/// auto fut = client.send_request(req);
/// typename decltype(client)::ResponseType resp;
/// NROS_TRY(fut.wait(executor.handle(), 5000, resp));
/// ```
template <typename S> class PollClient {
  public:
    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    /// Send a request and return a Future for the response (non-blocking).
    ///
    /// Call `wait()` on the returned future to block until the response
    /// arrives, or poll with `is_ready()` / `try_take()`.
    ///
    /// @param req  Request to send.
    /// @return Future that resolves to the response. Returns a consumed
    ///         (empty) future on serialization or send failure.
    ::nros::Future<ResponseType> send_request(const RequestType& req) {
        return send_request_sized<::nros::rx_buffer_capacity<ResponseType>::value>(req);
    }

    /// @ref send_request with the REPLY buffer sized by the caller.
    ///
    /// The receive buffer of a `Future<T>` is a member, so the capacity is a
    /// class template argument rather than a function one: this returns a
    /// `Future<ResponseType, RespCap>` (issue 0964). The request buffer is a
    /// transmit scratch buffer and is deliberately left on the estimate --
    /// over-sizing there only wastes stack.
    ///
    /// @tparam RespCap  Stack bytes the returned future holds for the reply.
    template <size_t RespCap>
    ::nros::Future<ResponseType, RespCap> send_request_sized(const RequestType& req) {
        using Fut = ::nros::Future<ResponseType, RespCap>;
        if (!initialized_) return Fut();

        uint8_t req_buf[::nros::detail::buffer_bounds<RequestType>::tx];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Fut();
        }

        nros_cpp_ret_t ret = nros_cpp_service_client_send_request(storage_, req_buf, req_len);
        if (ret != 0) return Fut();

        return Fut(storage_, &nros_cpp_service_client_take_response,
                   0 // slot 0 (single outstanding request)
        );
    }

    /// Send a request and block until a reply is received.
    ///
    /// Spins the executor internally (like the runtime's `Promise::wait`).
    /// Never calls `zpico_get` — all I/O is driven by `spin_once`.
    ///
    /// @param req          Request to send.
    /// @param resp         Output response struct (filled on success).
    /// @param timeout_ms   Maximum wait time (default 5000ms).
    /// @return Result indicating success, timeout, or failure.
    Result call(const RequestType& req, ResponseType& resp, uint32_t timeout_ms = 5000) {
        return call_sized<::nros::rx_buffer_capacity<ResponseType>::value>(req, resp, timeout_ms);
    }

    /// @ref call with the REPLY buffer sized by the caller (issue 0964).
    template <size_t RespCap>
    Result call_sized(const RequestType& req, ResponseType& resp, uint32_t timeout_ms = 5000) {
        if (!initialized_ || !executor_) return Result(::nros::ErrorCode::NotInitialized);
        auto fut = send_request_sized<RespCap>(req);
        return fut.wait(executor_, timeout_ms, resp);
    }

    /// Issue 0278 (Half B) — send a request and block up to `timeout_ms` for the
    /// reply WITHOUT spinning the executor, so this is safe to call from inside
    /// a subscription/timer callback (where `call()`/`Future::wait` would return
    /// `Reentrant`, issue 0290). It sends then sleep-polls the reply queue.
    ///
    /// CONSTRAINT: usable from a callback only on a MULTI-THREADED backend
    /// (zenoh MT, cyclonedds), where the backend's own read task delivers the
    /// reply into the client's queue while this loop yields. On a
    /// single-threaded / polled backend the reply can only arrive via
    /// `spin_once` — which the callback is blocking — so it will TIME OUT; use
    /// `call()` from the main loop there. Keep `timeout_ms` SHORT (tens of ms):
    /// this blocks the executor's dispatch thread for its duration.
    ///
    /// @param req          Request to send.
    /// @param resp         Output response struct (filled on success).
    /// @param timeout_ms   Maximum wait (default 100ms).
    /// @return success on a received reply; ErrorCode::Timeout on no reply in
    ///         time; NotInitialized / Error otherwise.
    Result call_polling(const RequestType& req, ResponseType& resp, uint32_t timeout_ms = 100) {
        return call_polling_sized<::nros::rx_buffer_capacity<ResponseType>::value>(req, resp,
                                                                                   timeout_ms);
    }

    /// @ref call_polling with the REPLY buffer sized by the caller (issue
    /// 0964). The request buffer stays on the estimate: it is transmit
    /// scratch, where an over-estimate only wastes stack.
    template <size_t RespCap>
    Result call_polling_sized(const RequestType& req, ResponseType& resp,
                              uint32_t timeout_ms = 100) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t req_buf[::nros::detail::buffer_bounds<RequestType>::tx];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Result(::nros::ErrorCode::Error);
        }
        uint8_t resp_buf[RespCap];
        size_t resp_len = 0;
        nros_cpp_ret_t ret = nros_cpp_service_client_call_raw(
            storage_, req_buf, req_len, resp_buf, sizeof(resp_buf), &resp_len, timeout_ms);
        if (ret != 0) return Result(ret);
        if (resp_len == 0) return Result(::nros::ErrorCode::Timeout);
        if (ResponseType::ffi_deserialize(resp_buf, resp_len, &resp) != 0) {
            return Result(::nros::ErrorCode::Error);
        }
        return Result::success();
    }

    /// Check if the client is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Read back the service name this client was created on — phase-444.
    ///
    /// See @ref rclcpp::Client::get_service_name, which is the same accessor on
    /// the dispatch half and states why each half keeps its own copy of the name
    /// rather than sharing one.
    const char* get_service_name() const { return initialized_ ? service_name_ : ""; }

    /// The QoS the backend GRANTED this client's REQUEST endpoint — the
    /// publisher that sends calls. Issue 1437.
    ///
    /// ONE create builds TWO endpoints that negotiate against DIFFERENT peers,
    /// so this and @ref get_response_subscription_actual_qos are two answers and
    /// neither stands for the other. A policy the backend cannot report is an
    /// ABSENCE (`ReliabilityUnknown` and friends), never the request echoed
    /// back — see @ref Publisher::get_actual_qos.
    ///
    /// This half OWNS its `RmwServiceClient`, so it passes `storage_` and the
    /// `callback_mode_` branch issue 1437 needed is gone. See
    /// @ref rclcpp::Client::get_request_publisher_actual_qos for why the
    /// dispatch half answers too — the client comes out the way the SERVICE did
    /// and not the way the subscription did, because issue 1437 gave both
    /// service halves an `executor_` beside the index.
    ///
    /// UPSTREAM PARITY: upstream has no poll-style service client, so the method
    /// name is borrowed from `rclcpp::Client` on the type that is ours.
    ::nros::QoS get_request_publisher_actual_qos() const { return actual_qos_half(true); }

    /// The QoS the backend GRANTED this client's RESPONSE endpoint — the
    /// subscription that receives replies. Issue 1437; see
    /// @ref get_request_publisher_actual_qos.
    ::nros::QoS get_response_subscription_actual_qos() const { return actual_qos_half(false); }

    /// Phase 124.C.3 — graph-aware "is the matching server up?" probe.
    ///
    /// Returns the count from the RMW backend's matched-server view:
    /// * `1`  — at least one matching server is currently visible.
    /// * `ok(false)` — no matching server discovered yet.
    /// * `error(Unsupported)` — backend cannot answer (e.g. XRCE without
    ///           participant enumeration); caller must fall back to a timed
    ///           `wait_for_service` or assume reachability.
    /// * `error(<code>)` — the probe itself failed.
    ///
    /// Never spins the executor — synchronous, safe to call from
    /// inside callbacks. Mirrors `rclcpp::ClientBase::service_is_ready`
    /// but with a tri-state result instead of collapsing
    /// "don't know" and "no" into the same `false`.
    ///
    /// phase-456 W9 — on THIS half only. `nros_cpp_service_client_server_available`
    /// reads the `RmwServiceClient` in `storage_`, which a dispatch client does
    /// not have; ledgered at `cpp:Client::service_is_ready`.
    ::nros::ResultOf<bool> service_is_ready() const {
        if (!initialized_) return ::nros::ResultOf<bool>::error(::nros::ErrorCode::NotInitialized);
        int out = -1;
        nros_cpp_ret_t ret =
            nros_cpp_service_client_server_available(const_cast<uint8_t*>(storage_), &out);
        // A failed CALL and a backend that cannot ANSWER are different facts,
        // and the old `int` form reported both as `-1`. Keep them apart.
        if (ret != 0) return ::nros::ResultOf<bool>::error(static_cast<::nros::ErrorCode>(ret));
        if (out < 0) return ::nros::ResultOf<bool>::error(::nros::ErrorCode::Unsupported);
        return ::nros::ResultOf<bool>::ok(out != 0);
    }

    /// @deprecated Use `service_is_ready()`.
    ///
    /// phase-379 W6 — preserved exactly: `1` ready, `0` not yet, `-1` cannot
    /// answer. It cannot distinguish a failed call from an unsupported backend,
    /// which is why it is replaced rather than kept.
    [[deprecated("PollClient::server_available is deprecated; use "
                 "PollClient::service_is_ready, which returns ::nros::ResultOf<bool>")]] int
    server_available() const {
        auto r = service_is_ready();
        if (!r.ok()) return -1;
        return r.value() ? 1 : 0;
    }

    /// phase-338 W8 — block until a matching service server is discoverable.
    ///
    /// Mirrors `rclcpp::ClientBase::wait_for_service`. Prefer this over
    /// hand-rolling a retry loop around the first `call()` / `send_request()`:
    /// it waits for the actual condition instead of guessing an attempt count,
    /// and it re-probes, so a server that starts AFTER the wait begins is still
    /// seen (a single liveliness query samples the router's current token list
    /// and terminates).
    ///
    /// Spins the executor cooperatively while probing, so do NOT call it from
    /// inside a callback — use the non-blocking `service_is_ready()` there.
    ///
    /// Returns ok when the server is visible, `Timeout` when the budget
    /// elapses.
    ///
    /// **The budget is REQUIRED** — phase-417 stage 3. Upstream's default is
    /// `-1`, WAIT FOREVER; this call cannot (RFC-0021: it drives the executor
    /// cooperatively, and a wait that never returns starves every other entity
    /// on a single-threaded transport), and `uint32_t` has no value to port -1
    /// to. It used to default to 5000, so a ported argument-free
    /// `client->wait_for_service()` returned `Timeout` after five seconds where
    /// upstream was still waiting — and `[[nodiscard]]` does not catch
    /// `if (!client->wait_for_service())`, which is what upstream code writes.
    /// The no-argument form is now a compile error carrying
    /// `NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT`.
    ///
    /// phase-456 W9 — on THIS half only, for the reason
    /// @ref service_is_ready gives: the FFI takes caller storage and says so.
    Result wait_for_service(uint32_t timeout_ms) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        return Result(nros_cpp_service_client_wait_for_service(storage_, executor_, timeout_ms));
    }

    /// **REFUSED** — `wait_for_service()` with no budget. phase-417 stage 3.
    ///
    /// A member template rather than `= delete` for the C++14 reason given on
    /// `Executor::spin_once()`: a deleted function carries no message there.
    template <typename T = void> Result wait_for_service() {
        static_assert(::rclcpp::detail::refuse<T>::value, NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT);
        return Result(::nros::ErrorCode::Unsupported);
    }

    /// Destructor -- releases the service client.
    ///
    /// Unconditional since phase-456 W9: every `PollClient<S>` owns an
    /// `RmwServiceClient` in `storage_`. The `!callback_mode_` guard this
    /// replaced existed because one class served two owners; the dispatch entity
    /// is `rclcpp::Client<S>` now and has no storage to free.
    ~PollClient() {
        if (initialized_) {
            nros_cpp_service_client_destroy(storage_);
        }
        initialized_ = false;
    }

    // Move semantics (non-copyable). Relocation goes through the
    // `nros_cpp_service_client_relocate` runtime call (Phase 84.C1).
    PollClient(PollClient&& other)
        : executor_(other.executor_), initialized_(other.initialized_), service_name_{} {
        ::nros::detail::assign_entity_name(service_name_, other.service_name_);
        if (other.initialized_) {
            nros_cpp_service_client_relocate(other.storage_, storage_);
        }
        other.initialized_ = false;
    }

    PollClient& operator=(PollClient&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_service_client_destroy(storage_);
            }
            executor_ = other.executor_;
            initialized_ = other.initialized_;
            ::nros::detail::assign_entity_name(service_name_, other.service_name_);
            if (other.initialized_) {
                nros_cpp_service_client_relocate(other.storage_, storage_);
            }
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor -- creates an uninitialized service client.
    /// Use `Node::create_client()` to initialize.
    PollClient() : storage_(), executor_(nullptr), initialized_(false), service_name_{} {}

  private:
    PollClient(const PollClient&) = delete;
    PollClient& operator=(const PollClient&) = delete;

    friend class ::rclcpp::Node;

    /// The two directions differ only in which out-pointer is read, so one
    /// body serves both and they cannot end up swapped (issue 1437).
    ///
    /// `executor` is always NULL and `handle_id` always 0 here: this half owns
    /// its client, so `storage_` is the road, and the FFI takes exactly one of
    /// the two.
    ::nros::QoS actual_qos_half(bool request) const {
        nros_cpp_qos_t req{};
        nros_cpp_qos_t resp{};
        if (!initialized_) return ::nros::detail::qos_all_unknown();
        if (nros_cpp_service_client_get_actual_qos(static_cast<const void*>(storage_), nullptr, 0,
                                                   &req, &resp) != 0) {
            return ::nros::detail::qos_all_unknown();
        }
        return ::nros::detail::qos_from_ffi(request ? req : resp);
    }

    alignas(8) uint8_t storage_[NROS_SERVICE_CLIENT_SIZE];
    void* executor_;
    bool initialized_;
    /// phase-444 — the service name, kept C++-side for `get_service_name()`.
    /// See `rclcpp::Client`'s field of the same name.
    char service_name_[::nros::SERVICE_NAME_MAX];
};

} // namespace nros

// Out-of-line definition of the future-style `Node::create_client<S>()`. Placed
// after the class for the reason Phase 84.G8 gives for every entity: a consumer
// pays for this template only when it uses a poll client.
#include "nros/node.hpp"

namespace rclcpp {
template <typename S>
Result Node::create_client(::nros::PollClient<S>& out, const char* service_name,
                           const ::nros::QoS& qos) {
    if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::nros::detail::qos_to_ffi(qos);
    nros_cpp_ret_t ret = nros_cpp_service_client_create(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos, out.storage_);
    if (ret == 0) {
        out.executor_ = executor_handle_;
        // phase-444 — remember the name for `get_service_name()`; the runtime
        // takes `service_name` and drops it.
        ::nros::detail::assign_entity_name(out.service_name_, service_name);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

#endif // NROS_CPP_POLLING_CLIENT_HPP
