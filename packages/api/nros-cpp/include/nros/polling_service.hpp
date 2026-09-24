// nros-cpp: the poll-style service server
// Freestanding C++ — no exceptions, no STL required

/**
 * @file polling_service.hpp
 * @ingroup grp_service
 * @brief `nros::PollService<S>` — the service server the CALLER owns and drains.
 *
 * WHICH ONE DO I WANT
 *
 *   `PollService<S>`      — the caller owns an `RmwServiceServer` in its own
 *                           storage, drives `spin_once()` itself, and drains
 *                           with `take_request()` / `send_response()`. This is
 *                           the type that carries the taking API.
 *   `rclcpp::Service<S>`  — the DISPATCH server (`nros/service.hpp`). A handler
 *                           is registered into the executor arena, which owns
 *                           the server and runs the handler during spin. The
 *                           caller invokes nothing on it.
 *
 * WHY THE TAKING API LIVES HERE — phase-456 W5, following W2b
 *
 * It used to live on `rclcpp::Service<S>`, which served BOTH ownership models
 * behind a `callback_mode_` flag. On the dispatch path the arena owns the
 * server and `storage_` is never filled, so `take_request()` on a
 * callback-mode object handed `NROS_SERVICE_SERVER_SIZE` value-initialized
 * zero bytes to `nros_cpp_service_server_take_request_raw`. Nothing on that
 * path checked — `initialized_` is true for both. The split is what makes the
 * call UNWRITABLE rather than merely discouraged, which is the same reason and
 * the same remedy W2b applied to `nros::PollSubscription<M>`.
 *
 * It is also what unblocks the alias. `Node::create_service<S>(name, qos)`
 * with no handler returns and exists to be `->take_request()`'d, so while one
 * class served both, `Service<S>::SharedPtr` had to be both a handle and a
 * pointer to a poll object. W3 recorded that as the blocker; this is its
 * removal.
 *
 * UPSTREAM PARITY, STATED: upstream has no poll-style service server — a
 * `rclcpp::Service` belongs to the node and its handler is required. So this
 * type is OURS and says so, and `take_request` / `send_response` are its names
 * rather than a divergence on upstream's.
 */

#ifndef NROS_CPP_POLLING_SERVICE_HPP
#define NROS_CPP_POLLING_SERVICE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/config.hpp"
#include "nros/entity_name.hpp" // phase-444 — the one entity-name copy
#include "nros/owned.hpp"
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

/// Poll-style service server — caller-owned storage, consuming receive.
///
/// phase-456 W5: this is the class `rclcpp::Service<S>` used to be when it was
/// created through the out-ref `create_service(out, name, qos)`.
///
/// Usage:
/// ```cpp
/// nros::PollService<example_interfaces::srv::AddTwoInts> srv;
/// NROS_TRY(node.create_service(srv, "/add_two_ints"));
/// typename decltype(srv)::RequestType req;
/// int64_t seq;
/// if (srv.take_request(req, seq).ok()) {
///     typename decltype(srv)::ResponseType resp;
///     resp.sum = req.a + req.b;
///     srv.send_response(seq, resp);
/// }
/// ```
template <typename S> class PollService {
  public:
    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    /// Try to receive a typed request (non-blocking).
    ///
    /// @param req     Output request struct (filled on success).
    /// @param seq_id  Output sequence number for reply matching.
    /// @return Result::success() if a request was received and deserialized;
    ///         ErrorCode::TryAgain if no data is available;
    ///         ErrorCode::NotInitialized or the FFI error code otherwise;
    ///         ErrorCode::Error if deserialization failed.
    Result take_request(RequestType& req, int64_t& seq_id) {
        return try_recv_request_sized<::nros::rx_buffer_capacity<RequestType>::value>(req, seq_id);
    }

    /// @ref take_request with the receive buffer sized by the CALLER.
    ///
    /// This IS a receive buffer — issue 0964's survey listed the service
    /// request under "transmit", which is true of `Client<S>`'s request and
    /// false here: the server deserializes out of it, so an under-estimate
    /// truncates. See @ref PollSubscription::take_sized.
    template <size_t Cap> Result try_recv_request_sized(RequestType& req, int64_t& seq_id) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t buf[Cap];
        size_t len = 0;
        int64_t seq = 0;
        nros_cpp_ret_t ret =
            nros_cpp_service_server_take_request_raw(storage_, buf, sizeof(buf), &len, &seq);
        if (ret != 0) return Result(ret);
        if (len == 0) return Result(::nros::ErrorCode::TryAgain);
        if (RequestType::ffi_deserialize(buf, len, &req) != 0)
            return Result(::nros::ErrorCode::Error);
        seq_id = seq;
        return Result::success();
    }

    /// @deprecated Use `take_request(RequestType&, int64_t&)`.
    ///
    /// phase-379 W6 decision 1 (2026-09-03): `try_recv` -> `take`. rcl
    /// (`rcl_take_request`), rclcpp (`Service::take_request`) and our own RMW
    /// vtable (`take_request`) already said `take`; only this layer said
    /// `try_recv`. Header-only forwarder, no ABI cost. Scheduled for removal.
    [[deprecated(
        "PollService::try_recv_request is deprecated; use PollService::take_request")]] Result
    try_recv_request(RequestType& req, int64_t& seq_id) {
        return take_request(req, seq_id);
    }

    /// Send a typed reply to a previously received request.
    ///
    /// @param seq_id  Sequence number from take_request().
    /// @param resp    Response to send.
    /// @return Result indicating success or failure.
    Result send_response(int64_t seq_id, const ResponseType& resp) {
        if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
        uint8_t buf[::nros::detail::buffer_bounds<ResponseType>::tx];
        size_t len = 0;
        if (ResponseType::ffi_serialize(&resp, buf, sizeof(buf), &len) != 0) {
            return Result(::nros::ErrorCode::Error);
        }
        return Result(nros_cpp_service_server_send_response_raw(storage_, seq_id, buf, len));
    }

    /// @deprecated Use `send_response()`.
    ///
    /// Phase-379 W5: rcl, rclcpp and rclrs all say `send_response`, and our
    /// own C already used that word. Kept as a forwarder so an out-of-tree
    /// node on the old spelling still compiles and is told what to move to.
    [[deprecated(
        "PollService::send_reply() is deprecated; use PollService::send_response()")]] Result
    send_reply(int64_t seq_id, const ResponseType& resp) {
        return send_response(seq_id, resp);
    }

    /// Check if the service is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Read back the service name this server was created on — phase-444.
    ///
    /// See @ref rclcpp::Service::get_service_name, which is the same accessor
    /// on the dispatch half and states why each half keeps its own copy of the
    /// name rather than sharing one.
    const char* get_service_name() const { return initialized_ ? service_name_ : ""; }

    /// The QoS the backend GRANTED this service's REQUEST endpoint — the
    /// subscription that receives calls. Issue 1437.
    ///
    /// ONE create builds TWO endpoints that negotiate against DIFFERENT peers,
    /// so this and @ref get_response_publisher_actual_qos are two answers and
    /// neither stands for the other. A policy the backend cannot report is an
    /// ABSENCE (`ReliabilityUnknown` and friends), never the request echoed
    /// back — see @ref Publisher::get_actual_qos.
    ///
    /// This half OWNS its `RmwServiceServer`, so it passes `storage_` and the
    /// `callback_mode_` branch issue 1437 needed is gone. See
    /// @ref rclcpp::Service::get_request_subscription_actual_qos for why the
    /// dispatch half answers too, which is where the service differs from the
    /// subscription W2b split the same way.
    ///
    /// UPSTREAM PARITY: upstream has no poll-style service server, so the
    /// method name is borrowed from `rclcpp::Service` on the type that is ours.
    ::nros::QoS get_request_subscription_actual_qos() const { return actual_qos_half(true); }

    /// The QoS the backend GRANTED this service's RESPONSE endpoint — the
    /// publisher that sends replies. Issue 1437; see
    /// @ref get_request_subscription_actual_qos.
    ::nros::QoS get_response_publisher_actual_qos() const { return actual_qos_half(false); }

    /// Destructor — releases the service server.
    ///
    /// Unconditional since phase-456 W5: every `PollService<S>` owns an
    /// `RmwServiceServer` in `storage_`. The `!callback_mode_` guard this
    /// replaced existed because one class served two owners; the dispatch
    /// entity is `rclcpp::Service<S>` now and has no storage to free.
    ~PollService() {
        if (initialized_) {
            nros_cpp_service_server_destroy(storage_);
        }
        initialized_ = false;
    }

    // Move semantics (non-copyable). Relocation goes through the
    // `nros_cpp_service_server_relocate` runtime call (Phase 84.C1).
    PollService(PollService&& other) : initialized_(other.initialized_), service_name_{} {
        ::nros::detail::assign_entity_name(service_name_, other.service_name_);
        if (other.initialized_) {
            nros_cpp_service_server_relocate(other.storage_, storage_);
        }
        other.initialized_ = false;
    }

    PollService& operator=(PollService&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_service_server_destroy(storage_);
            }
            initialized_ = other.initialized_;
            ::nros::detail::assign_entity_name(service_name_, other.service_name_);
            if (other.initialized_) {
                nros_cpp_service_server_relocate(other.storage_, storage_);
            }
            other.initialized_ = false;
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized service server.
    /// Use `Node::create_service()` to initialize.
    PollService() : storage_(), initialized_(false), service_name_{} {}

  private:
    PollService(const PollService&) = delete;
    PollService& operator=(const PollService&) = delete;

    friend class ::rclcpp::Node;

    /// The two directions differ only in which out-pointer is read, so one
    /// body serves both and they cannot end up swapped (issue 1437).
    ///
    /// `executor` is always NULL and `handle_id` always 0 here: this half owns
    /// its server, so `storage_` is the road, and the FFI takes exactly one of
    /// the two.
    ::nros::QoS actual_qos_half(bool request) const {
        nros_cpp_qos_t req{};
        nros_cpp_qos_t resp{};
        if (!initialized_) return ::nros::detail::qos_all_unknown();
        if (nros_cpp_service_server_get_actual_qos(static_cast<const void*>(storage_), nullptr, 0,
                                                   &req, &resp) != 0) {
            return ::nros::detail::qos_all_unknown();
        }
        return ::nros::detail::qos_from_ffi(request ? req : resp);
    }

    alignas(8) uint8_t storage_[NROS_SERVICE_SERVER_SIZE];
    bool initialized_;
    /// phase-444 — the service name, kept C++-side for `get_service_name()`.
    /// See `Client`'s field of the same name.
    char service_name_[::nros::SERVICE_NAME_MAX];
};

} // namespace nros

// Out-of-line definition of the poll-style `Node::create_service<S>()`. Placed
// after the class for the reason Phase 84.G8 gives for every entity: a consumer
// pays for this template only when it uses a poll service.
#include "nros/node.hpp"

namespace rclcpp {
template <typename S>
Result Node::create_service(::nros::PollService<S>& out, const char* service_name,
                            const ::nros::QoS& qos) {
    if (!initialized_) return Result(::nros::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::nros::detail::qos_to_ffi(qos);
    nros_cpp_ret_t ret = nros_cpp_service_server_create(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos, out.storage_);
    if (ret == 0) {
        // phase-444 — remember the name for `get_service_name()`; the runtime
        // takes `service_name` and drops it.
        ::nros::detail::assign_entity_name(out.service_name_, service_name);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

#endif // NROS_CPP_POLLING_SERVICE_HPP
