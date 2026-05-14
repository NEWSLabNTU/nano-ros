// nros-cpp: Service client class
// Freestanding C++ -- no exceptions, no STL required

/**
 * @file client.hpp
 * @ingroup grp_service
 * @brief `nros::Client<S>` — typed service client.
 */

#ifndef NROS_CPP_CLIENT_HPP
#define NROS_CPP_CLIENT_HPP

#include <cstdint>
#include <cstddef>

#include "nros/config.hpp"
#include "nros/result.hpp"
#include "nros/future.hpp"

#include "nros_cpp_ffi.h"

namespace nros {

/// Typed service client for a ROS 2 service.
///
/// Mirrors `rclcpp::Client<S>`. The service type `S` must provide
/// nested `Request` and `Response` types with `TYPE_NAME`, `TYPE_HASH`,
/// `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Usage (async -- preferred):
/// ```cpp
/// nros::Client<example_interfaces::srv::AddTwoInts> client;
/// NROS_TRY(node.create_client(client, "/add_two_ints"));
/// auto fut = client.send_request(req);
/// ResponseType resp;
/// NROS_TRY(fut.wait(executor.handle(), 5000, resp));
/// ```
template <typename S> class Client {
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
    Future<ResponseType> send_request(const RequestType& req) {
        if (!initialized_) return Future<ResponseType>();

        uint8_t req_buf[RequestType::SERIALIZED_SIZE_MAX];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Future<ResponseType>();
        }

        nros_cpp_ret_t ret = nros_cpp_service_client_send_request(storage_, req_buf, req_len);
        if (ret != 0) return Future<ResponseType>();

        return Future<ResponseType>(storage_, &nros_cpp_service_client_try_recv_reply,
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
        if (!initialized_ || !executor_) return Result(ErrorCode::NotInitialized);
        auto fut = send_request(req);
        return fut.wait(executor_, timeout_ms, resp);
    }

    /// Check if the client is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Phase 124.C.3 — graph-aware "is the matching server up?" probe.
    ///
    /// Returns the count from the RMW backend's matched-server view:
    /// * `1`  — at least one matching server is currently visible.
    /// * `0`  — no matching server discovered yet.
    /// * `-1` — backend cannot answer (e.g. XRCE without participant
    ///           enumeration); caller must fall back to a timed
    ///           `wait_for_service` or assume reachability.
    ///
    /// Never spins the executor — synchronous, safe to call from
    /// inside callbacks. Mirrors `rclcpp::ClientBase::service_is_ready`
    /// but with a tri-state result instead of collapsing
    /// "don't know" and "no" into the same `false`.
    int server_available() const {
        if (!initialized_) return -1;
        int out = -1;
        nros_cpp_ret_t ret = nros_cpp_service_client_server_available(
            const_cast<uint8_t*>(storage_), &out);
        if (ret != 0) return -1;
        return out;
    }

    /// Destructor -- releases service client resources.
    ~Client() {
        if (initialized_) {
            nros_cpp_service_client_destroy(storage_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable). Relocation goes through the
    // `nros_cpp_service_client_relocate` runtime call (Phase 84.C1).
    Client(Client&& other) : executor_(other.executor_), initialized_(other.initialized_) {
        if (other.initialized_) {
            nros_cpp_service_client_relocate(other.storage_, storage_);
            other.initialized_ = false;
        }
    }

    Client& operator=(Client&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_service_client_destroy(storage_);
                initialized_ = false;
            }
            executor_ = other.executor_;
            if (other.initialized_) {
                nros_cpp_service_client_relocate(other.storage_, storage_);
                initialized_ = true;
                other.initialized_ = false;
            }
        }
        return *this;
    }

    /// Default constructor -- creates an uninitialized service client.
    /// Use `Node::create_client()` to initialize.
    Client() : storage_(), executor_(nullptr), initialized_(false) {}

  private:
    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    friend class Node;

    alignas(8) uint8_t storage_[NROS_SERVICE_CLIENT_SIZE];
    void* executor_;
    bool initialized_;
};

} // namespace nros

// Phase 84.G8: out-of-line definition of Node::create_client<S>().
#include "nros/node.hpp"

namespace nros {

template <typename S>
Result Node::create_client(Client<S>& out, const char* service_name, const QoS& qos) {
    if (!initialized_) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos;
    ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
    ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
    ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
    ffi_qos.liveliness_kind = static_cast<nros_cpp_qos_liveliness_t>(qos.liveliness_raw());
    ffi_qos.depth = qos.depth();
    ffi_qos.deadline_ms = qos.deadline_ms();
    ffi_qos.lifespan_ms = qos.lifespan_ms();
    ffi_qos.liveliness_lease_ms = qos.liveliness_lease_ms();
    ffi_qos.avoid_ros_namespace_conventions = qos.avoid_ros_namespace_conventions() ? 1 : 0;
    nros_cpp_ret_t ret = nros_cpp_service_client_create(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos, out.storage_);
    if (ret == 0) {
        out.executor_ = executor_handle_;
        out.initialized_ = true;
    }
    return Result(ret);
}

} // namespace nros

#endif // NROS_CPP_CLIENT_HPP
