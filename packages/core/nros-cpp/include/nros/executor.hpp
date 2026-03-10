// nros-cpp: Executor class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_EXECUTOR_HPP
#define NROS_CPP_EXECUTOR_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;

struct nros_cpp_node_t;

nros_cpp_ret_t nros_cpp_init(const char* locator, uint8_t domain_id, const char* node_name,
                             const char* ns, void** out_handle);
nros_cpp_ret_t nros_cpp_fini(void* handle);
nros_cpp_ret_t nros_cpp_node_create(void* executor_handle, const char* name, const char* ns,
                                    nros_cpp_node_t* out_node);
nros_cpp_ret_t nros_cpp_spin_once(void* handle, int32_t timeout_ms);
} // extern "C"

namespace nros {

// Forward declarations
class Node;

/// Explicit executor for managing ROS 2 entities and spinning.
///
/// Mirrors `rclcpp::executors::SingleThreadedExecutor`. Provides an
/// explicit alternative to the global `nros::init()`/`nros::spin_once()`
/// free functions.
///
/// Usage:
/// ```cpp
/// nros::Executor executor;
/// NROS_TRY(nros::Executor::create(executor));
///
/// nros::Node node;
/// NROS_TRY(executor.create_node(node, "my_node"));
///
/// // Create publishers, subscriptions, etc. on node...
///
/// while (executor.ok()) {
///     executor.spin_once(10);
/// }
/// executor.shutdown();
/// ```
class Executor {
  public:
    /// Default constructor — creates an uninitialized executor.
    Executor() : handle_(nullptr) {}

    /// Create and initialize an executor.
    ///
    /// Opens a middleware connection. This is the explicit alternative
    /// to `nros::init()`.
    ///
    /// @param out        Receives the initialized executor.
    /// @param locator    Middleware locator (e.g., "tcp/127.0.0.1:7447"), or nullptr.
    /// @param domain_id  ROS domain ID (0-232).
    /// @return Result indicating success or failure.
    static Result create(Executor& out, const char* locator = nullptr, uint8_t domain_id = 0) {
        void* handle = nullptr;
        nros_cpp_ret_t ret = nros_cpp_init(locator, domain_id, "nros_cpp", nullptr, &handle);
        if (ret == 0) {
            out.handle_ = handle;
        }
        return Result(ret);
    }

    /// Create a node on this executor.
    ///
    /// @param out   Receives the initialized node.
    /// @param name  Node name (null-terminated).
    /// @param ns    Node namespace (null-terminated), or nullptr for "/".
    /// @return Result indicating success or failure.
    Result create_node(Node& out, const char* name, const char* ns = nullptr);

    /// Drive transport I/O and dispatch callbacks.
    ///
    /// Processes pending subscriptions, timers, services, and guard conditions.
    /// Call this periodically in your main loop.
    ///
    /// @param timeout_ms  Maximum time to block waiting for I/O.
    /// @return Result indicating success or failure.
    Result spin_once(int32_t timeout_ms = 10) {
        if (!handle_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_spin_once(handle_, timeout_ms));
    }

    /// Spin for a duration (blocking).
    ///
    /// Repeatedly calls `spin_once()` until `duration_ms` has elapsed.
    ///
    /// @param duration_ms  Total time to spin, in milliseconds.
    /// @param poll_ms      Individual spin_once timeout (default: 10ms).
    /// @return Result from the last spin_once call.
    Result spin(uint32_t duration_ms, int32_t poll_ms = 10) {
        if (!handle_) return Result(ErrorCode::NotInitialized);
        uint32_t elapsed = 0;
        Result last = Result::success();
        while (elapsed < duration_ms) {
            int32_t remaining = static_cast<int32_t>(duration_ms - elapsed);
            int32_t timeout = remaining < poll_ms ? remaining : poll_ms;
            last = Result(nros_cpp_spin_once(handle_, timeout));
            if (!last.ok()) return last;
            elapsed += static_cast<uint32_t>(timeout);
        }
        return last;
    }

    /// Check if the executor is initialized.
    bool ok() const { return handle_ != nullptr; }

    /// Get the raw executor handle (for advanced use).
    void* handle() const { return handle_; }

    /// Shut down the executor and close the middleware connection.
    Result shutdown() {
        if (!handle_) return Result::success();
        nros_cpp_ret_t ret = nros_cpp_fini(handle_);
        handle_ = nullptr;
        return Result(ret);
    }

    /// Destructor — shuts down if still active.
    ~Executor() {
        if (handle_) {
            nros_cpp_fini(handle_);
            handle_ = nullptr;
        }
    }

    // Move semantics (non-copyable)
    Executor(Executor&& other) : handle_(other.handle_) { other.handle_ = nullptr; }

    Executor& operator=(Executor&& other) {
        if (this != &other) {
            if (handle_) {
                nros_cpp_fini(handle_);
            }
            handle_ = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

  private:
    Executor(const Executor&) = delete;
    Executor& operator=(const Executor&) = delete;

    void* handle_;
};

} // namespace nros

#endif // NROS_CPP_EXECUTOR_HPP
