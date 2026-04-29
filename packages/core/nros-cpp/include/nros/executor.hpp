// nros-cpp: Executor class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file executor.hpp
 * @ingroup grp_executor
 * @brief `nros::Executor` — drives transport I/O and dispatches callbacks.
 */

#ifndef NROS_CPP_EXECUTOR_HPP
#define NROS_CPP_EXECUTOR_HPP

#include <cstdint>
#include <cstddef>

#include "nros/result.hpp"
#include "nros/nros_cpp_config_generated.h"

// FFI declarations
extern "C" {
typedef int nros_cpp_ret_t;

struct nros_cpp_node_t;

nros_cpp_ret_t nros_cpp_init(const char* locator, uint8_t domain_id, const char* node_name,
                             const char* ns, void* storage);
nros_cpp_ret_t nros_cpp_fini(void* storage);
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
/// The executor uses inline opaque storage — no heap allocation required.
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
    Executor() : storage_(), initialized_(false) {}

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
        return create(out, locator, domain_id, "nros_cpp");
    }

    /// Create and initialize an executor with an explicit session name.
    ///
    /// `session_name` flows through to the XRCE-DDS RMW backend as the
    /// per-process key derivation seed. Two processes sharing one
    /// XRCE Agent MUST use distinct names; see `nros::init`'s named
    /// overload for the full discussion.
    static Result create(Executor& out, const char* locator, uint8_t domain_id,
                         const char* session_name) {
        // -3 = NROS_CPP_RET_INVALID_ARGUMENT (cbindgen header).
        if (session_name == nullptr) {
            return Result(-3);
        }
        nros_cpp_ret_t ret =
            nros_cpp_init(locator, domain_id, session_name, nullptr, out.storage_);
        if (ret == 0) {
            out.initialized_ = true;
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
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_spin_once(storage_, timeout_ms));
    }

    /// Spin for a duration (blocking).
    ///
    /// Repeatedly calls `spin_once()` until `duration_ms` has elapsed.
    ///
    /// @param duration_ms  Total time to spin, in milliseconds.
    /// @param poll_ms      Individual spin_once timeout (default: 10ms).
    /// @return Result from the last spin_once call.
    Result spin(uint32_t duration_ms, int32_t poll_ms = 10) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        uint32_t elapsed = 0;
        Result last = Result::success();
        while (elapsed < duration_ms) {
            int32_t remaining = static_cast<int32_t>(duration_ms - elapsed);
            int32_t timeout = remaining < poll_ms ? remaining : poll_ms;
            last = Result(nros_cpp_spin_once(storage_, timeout));
            if (!last.ok()) return last;
            elapsed += static_cast<uint32_t>(timeout);
        }
        return last;
    }

    /// Check if the executor is initialized.
    bool ok() const { return initialized_; }

    /// Get the raw executor storage (for advanced use).
    ///
    /// Non-const: downstream FFI mutates executor state through this
    /// pointer (e.g. `spin_once`), so exposing it as `const` would be a
    /// lie. Callers that only need to observe the handle should do so
    /// through methods on `Executor` directly.
    void* handle() { return storage_; }

    /// Shut down the executor and close the middleware connection.
    Result shutdown() {
        if (!initialized_) return Result::success();
        nros_cpp_ret_t ret = nros_cpp_fini(storage_);
        initialized_ = false;
        return Result(ret);
    }

    /// Destructor — shuts down if still active.
    ~Executor() {
        if (initialized_) {
            nros_cpp_fini(storage_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    Executor(Executor&& other) : initialized_(other.initialized_) {
        for (unsigned i = 0; i < sizeof(storage_); ++i) {
            storage_[i] = other.storage_[i];
            other.storage_[i] = 0;
        }
        other.initialized_ = false;
    }

    Executor& operator=(Executor&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_fini(storage_);
            }
            for (unsigned i = 0; i < sizeof(storage_); ++i) {
                storage_[i] = other.storage_[i];
                other.storage_[i] = 0;
            }
            initialized_ = other.initialized_;
            other.initialized_ = false;
        }
        return *this;
    }

  private:
    Executor(const Executor&) = delete;
    Executor& operator=(const Executor&) = delete;

    alignas(8) uint8_t storage_[NROS_CPP_EXECUTOR_STORAGE_SIZE];
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_EXECUTOR_HPP
