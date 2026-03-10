// nros-cpp: Service server class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_SERVICE_HPP
#define NROS_CPP_SERVICE_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_service_server_try_recv_raw(void* handle, uint8_t* out_data,
                                                    size_t out_capacity, size_t* out_len,
                                                    int64_t* out_sequence);
nros_cpp_ret_t nros_cpp_service_server_send_reply_raw(void* handle, int64_t sequence_number,
                                                      const uint8_t* data, size_t len);
nros_cpp_ret_t nros_cpp_service_server_destroy(void* handle);
} // extern "C"

namespace nros {

/// Typed service server for a ROS 2 service.
///
/// Mirrors `rclcpp::Service<S>`. The service type `S` must provide
/// nested `Request` and `Response` types with `TYPE_NAME`, `TYPE_HASH`,
/// `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Usage:
/// ```cpp
/// nros::Service<example_interfaces::srv::AddTwoInts> srv;
/// NROS_TRY(node.create_service(srv, "/add_two_ints"));
/// typename decltype(srv)::RequestType req;
/// int64_t seq;
/// if (srv.try_recv_request(req, seq)) {
///     typename decltype(srv)::ResponseType resp;
///     resp.sum = req.a + req.b;
///     srv.send_reply(seq, resp);
/// }
/// ```
template <typename S> class Service {
  public:
    using RequestType = typename S::Request;
    using ResponseType = typename S::Response;

    /// Try to receive a typed request (non-blocking).
    ///
    /// @param req     Output request struct (filled on success).
    /// @param seq_id  Output sequence number for reply matching.
    /// @return true if a request was received and deserialized.
    bool try_recv_request(RequestType& req, int64_t& seq_id) {
        if (!initialized_) return false;
        uint8_t buf[RequestType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        int64_t seq = 0;
        nros_cpp_ret_t ret =
            nros_cpp_service_server_try_recv_raw(handle_, buf, sizeof(buf), &len, &seq);
        if (ret != 0 || len == 0) return false;
        if (RequestType::ffi_deserialize(buf, len, &req) != 0) return false;
        seq_id = seq;
        return true;
    }

    /// Send a typed reply to a previously received request.
    ///
    /// @param seq_id  Sequence number from try_recv_request().
    /// @param resp    Response to send.
    /// @return Result indicating success or failure.
    Result send_reply(int64_t seq_id, const ResponseType& resp) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        uint8_t buf[ResponseType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (ResponseType::ffi_serialize(&resp, buf, sizeof(buf), &len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(nros_cpp_service_server_send_reply_raw(handle_, seq_id, buf, len));
    }

    /// Check if the service is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases service server resources.
    ~Service() {
        if (initialized_) {
            nros_cpp_service_server_destroy(handle_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    Service(Service&& other) : handle_(other.handle_), initialized_(other.initialized_) {
        other.handle_ = nullptr;
        other.initialized_ = false;
    }

    Service& operator=(Service&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_service_server_destroy(handle_);
            }
            handle_ = other.handle_;
            initialized_ = other.initialized_;
            other.handle_ = nullptr;
            other.initialized_ = false;
        }
        return *this;
    }

  private:
    Service(const Service&) = delete;
    Service& operator=(const Service&) = delete;

    friend class Node;
    Service() : handle_(nullptr), initialized_(false) {}

    void* handle_;
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_SERVICE_HPP
