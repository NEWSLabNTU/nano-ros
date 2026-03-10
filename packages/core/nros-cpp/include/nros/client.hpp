// nros-cpp: Service client class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_CLIENT_HPP
#define NROS_CPP_CLIENT_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_service_client_call_raw(void* handle, const uint8_t* req_data,
                                                size_t req_len, uint8_t* resp_data,
                                                size_t resp_capacity, size_t* resp_len);
nros_cpp_ret_t nros_cpp_service_client_destroy(void* handle);
} // extern "C"

namespace nros {

/// Typed service client for a ROS 2 service.
///
/// Mirrors `rclcpp::Client<S>`. The service type `S` must provide
/// nested `Request` and `Response` types with `TYPE_NAME`, `TYPE_HASH`,
/// `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Usage:
/// ```cpp
/// nros::Client<example_interfaces::srv::AddTwoInts> client;
/// NROS_TRY(node.create_client(client, "/add_two_ints"));
/// typename decltype(client)::RequestType req;
/// req.a = 1; req.b = 2;
/// typename decltype(client)::ResponseType resp;
/// NROS_TRY(client.call(req, resp));
/// // resp.sum == 3
/// ```
template <typename S> class Client {
  public:
    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    /// Send a request and block until a reply is received.
    ///
    /// @param req   Request to send.
    /// @param resp  Output response struct (filled on success).
    /// @return Result indicating success or failure.
    Result call(const RequestType& req, ResponseType& resp) {
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
        nros_cpp_ret_t ret = nros_cpp_service_client_call_raw(handle_, req_buf, req_len, resp_buf,
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

    /// Destructor — releases service client resources.
    ~Client() {
        if (initialized_) {
            nros_cpp_service_client_destroy(handle_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    Client(Client&& other) : handle_(other.handle_), initialized_(other.initialized_) {
        other.handle_ = nullptr;
        other.initialized_ = false;
    }

    Client& operator=(Client&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_service_client_destroy(handle_);
            }
            handle_ = other.handle_;
            initialized_ = other.initialized_;
            other.handle_ = nullptr;
            other.initialized_ = false;
        }
        return *this;
    }

  private:
    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    friend class Node;
    Client() : handle_(nullptr), initialized_(false) {}

    void* handle_;
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_CLIENT_HPP
