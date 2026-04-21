// nros-cpp: Service client class
// Freestanding C++ -- no exceptions, no STL required

#ifndef NROS_CPP_CLIENT_HPP
#define NROS_CPP_CLIENT_HPP

#include <cstdint>
#include <cstddef>

#include "nros/config.hpp"
#include "nros/result.hpp"
#include "nros/future.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_service_client_send_request(void* storage, const uint8_t* req_data,
                                                    size_t req_len);
nros_cpp_ret_t nros_cpp_service_client_try_recv_reply(void* storage, uint8_t* resp_data,
                                                      size_t resp_capacity, size_t* resp_len);
nros_cpp_ret_t nros_cpp_service_client_call_raw(void* storage, const uint8_t* req_data,
                                                size_t req_len, uint8_t* resp_data,
                                                size_t resp_capacity, size_t* resp_len);
nros_cpp_ret_t nros_cpp_service_client_destroy(void* storage);
nros_cpp_ret_t nros_cpp_service_client_relocate(void* old_storage, void* new_storage);
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

    /// Send a request and block until a reply is received (deprecated).
    ///
    /// Prefer `send_request()` + `Future::wait()` which allows the executor
    /// to spin while waiting.
    ///
    /// @param req   Request to send.
    /// @param resp  Output response struct (filled on success).
    /// @return Result indicating success or failure.
    [[deprecated("use send_request() + Future::wait(executor.handle(), timeout_ms, resp)")]] Result
    call(const RequestType& req, ResponseType& resp) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        // Serialize request
        uint8_t req_buf[RequestType::SERIALIZED_SIZE_MAX];
        size_t req_len = 0;
        if (RequestType::ffi_serialize(&req, req_buf, sizeof(req_buf), &req_len) != 0) {
            return Result(ErrorCode::Error);
        }

        // Call and receive reply
        uint8_t resp_buf[ResponseType::SERIALIZED_SIZE_MAX];
        size_t resp_len = 0;
        nros_cpp_ret_t ret = nros_cpp_service_client_call_raw(storage_, req_buf, req_len, resp_buf,
                                                              sizeof(resp_buf), &resp_len);
        if (ret != 0) return Result(ret);

        // Deserialize response
        if (ResponseType::ffi_deserialize(resp_buf, resp_len, &resp) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result::success();
    }

    /// Check if the client is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor -- releases service client resources.
    ~Client() {
        if (initialized_) {
            nros_cpp_service_client_destroy(storage_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable). Relocation goes through the
    // Rust-side `nros_cpp_service_client_relocate` FFI (Phase 84.C1).
    Client(Client&& other) : initialized_(other.initialized_) {
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
    Client() : storage_(), initialized_(false) {}

  private:
    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    friend class Node;

    alignas(8) uint8_t storage_[NROS_CPP_SERVICE_CLIENT_STORAGE_SIZE];
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_CLIENT_HPP
