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

    /// Phase 189.M3.3.f — typed response-handler signatures for the
    /// *callback-style* client (rclcpp async dispatch). The handler runs during
    /// `spin_once` when a reply arrives for a request sent via
    /// `async_send_request`.
    using TypedResponseFn = void (*)(const ResponseType& response);
    using TypedResponseFnWithCtx = void (*)(const ResponseType& response, void* ctx);

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
        nros_cpp_ret_t ret =
            nros_cpp_service_client_server_available(const_cast<uint8_t*>(storage_), &out);
        if (ret != 0) return -1;
        return out;
    }

    /// Phase 189.M3.3.f — callback-style async send. Only valid on a
    /// callback-style client (created via the `create_client(out, name, callback,
    /// ...)` overload); the reply is delivered to the registered response handler
    /// during `spin_once` (no Future). Returns immediately after sending.
    Result async_send_request(const RequestType& req) {
        if (!initialized_ || !callback_mode_) return Result(ErrorCode::NotInitialized);
        uint8_t req_buf[RequestType::SERIALIZED_SIZE_MAX];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(
            nros_cpp_service_client_send_on_handle(executor_, handle_id_, req_buf, req_len));
    }

    /// Executor handle for the callback-style client (Phase 189.M3.3.f);
    /// `SIZE_MAX` for future-style / uninitialized.
    size_t handle_id() const { return handle_id_; }

    /// Destructor -- releases service client resources.
    ///
    /// Future-style clients own an `RmwServiceClient` in `storage_`; callback-style
    /// clients (M3.3.f) are owned by the executor arena, so the dtor must NOT
    /// touch `storage_` for them.
    ~Client() {
        if (initialized_ && !callback_mode_) {
            nros_cpp_service_client_destroy(storage_);
        }
        initialized_ = false;
    }

    // Move semantics (non-copyable). Future-style relocation goes through the
    // `nros_cpp_service_client_relocate` runtime call (Phase 84.C1). A
    // callback-style client must NOT be moved after register — the arena holds
    // `this` as the response trampoline context (M3.3.f).
    Client(Client&& other)
        : executor_(other.executor_), initialized_(other.initialized_), user_fn_(other.user_fn_),
          user_fn_ctx_(other.user_fn_ctx_), user_ctx_(other.user_ctx_),
          handle_id_(other.handle_id_), callback_mode_(other.callback_mode_) {
        if (other.initialized_ && !other.callback_mode_) {
            nros_cpp_service_client_relocate(other.storage_, storage_);
        }
        other.initialized_ = false;
    }

    Client& operator=(Client&& other) {
        if (this != &other) {
            if (initialized_ && !callback_mode_) {
                nros_cpp_service_client_destroy(storage_);
            }
            executor_ = other.executor_;
            initialized_ = other.initialized_;
            user_fn_ = other.user_fn_;
            user_fn_ctx_ = other.user_fn_ctx_;
            user_ctx_ = other.user_ctx_;
            handle_id_ = other.handle_id_;
            callback_mode_ = other.callback_mode_;
            if (other.initialized_ && !other.callback_mode_) {
                nros_cpp_service_client_relocate(other.storage_, storage_);
            }
            other.initialized_ = false;
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

    /// Phase 189.M3.3.f — raw response trampoline matching `RawResponseCallback`
    /// (`void(data, len, ctx)`). Deserializes the reply, runs the user's typed
    /// handler. `ctx` is the `Client` object (`this`).
    static void response_trampoline(const uint8_t* data, size_t len, void* ctx) {
        auto* self = static_cast<Client*>(ctx);
        if (self == nullptr) return;
        ResponseType response;
        if (ResponseType::ffi_deserialize(data, len, &response) != 0) return;
        if (self->user_fn_ != nullptr) {
            self->user_fn_(response);
        } else if (self->user_fn_ctx_ != nullptr) {
            self->user_fn_ctx_(response, self->user_ctx_);
        }
    }

    alignas(8) uint8_t storage_[NROS_SERVICE_CLIENT_SIZE];
    void* executor_;
    bool initialized_;
    // Callback-style state (Phase 189.M3.3.f); unused in future mode.
    TypedResponseFn user_fn_ = nullptr;
    TypedResponseFnWithCtx user_fn_ctx_ = nullptr;
    void* user_ctx_ = nullptr;
    size_t handle_id_ = static_cast<size_t>(-1);
    bool callback_mode_ = false;
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
    ffi_qos.tx_express = qos.tx_express() ? 1 : 0;
    nros_cpp_ret_t ret = nros_cpp_service_client_create(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos, out.storage_);
    if (ret == 0) {
        out.executor_ = executor_handle_;
        out.initialized_ = true;
    }
    return Result(ret);
}

// Phase 189.M3.3.f — callback-style (arena-registered) client. The arena owns
// the client + dispatches `out`'s response handler during spin_once; requests
// go through `async_send_request`. `options.sched_context` is functional.
template <typename S, typename F, typename>
Result Node::create_client(Client<S>& out, const char* service_name, F callback, const QoS& qos,
                           const ClientOptions& options) {
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
    ffi_qos.tx_express = qos.tx_express() ? 1 : 0;

    out.user_fn_ = typename Client<S>::TypedResponseFn(callback);
    out.user_fn_ctx_ = nullptr;
    out.user_ctx_ = nullptr;

    uint8_t sched = (options.sched_context == SCHED_CONTEXT_UNSET)
                        ? 0u
                        : static_cast<uint8_t>(options.sched_context);
    size_t handle = static_cast<size_t>(-1);
    nros_cpp_ret_t ret = nros_cpp_service_client_register(
        &handle_, service_name, S::TYPE_NAME, S::Request::TYPE_HASH, ffi_qos,
        reinterpret_cast<nros_cpp_service_response_callback_t>(&Client<S>::response_trampoline),
        &out, sched, &handle);
    if (ret == 0) {
        out.executor_ = executor_handle_;
        out.handle_id_ = handle;
        out.callback_mode_ = true;
        out.initialized_ = true;
    }
    return Result(ret);
}

} // namespace nros

#endif // NROS_CPP_CLIENT_HPP
