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

#include "nros_cpp_ffi.h"

namespace nros {

// Forward declarations
class Node;
class NodeBuilder;

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
        // -3 = NROS_CPP_RET_INVALID_ARGUMENT (generated header).
        if (session_name == nullptr) {
            return Result(-3);
        }
        nros_cpp_ret_t ret = nros_cpp_init(locator, domain_id, session_name, nullptr, out.storage_);
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

    /// Phase 104.C.9 — chainable Node-creation builder.
    ///
    /// Mirrors Rust's `Executor::node_builder(name).rmw(...).locator(...)
    /// .domain_id(...).namespace(...).sched(...).build()`. Use this when
    /// binding a Node to a specific RMW backend, locator, domain, or
    /// SchedContext. Definition follows the full `NodeBuilder` class in
    /// `node.hpp`.
    NodeBuilder node_builder(const char* name);

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

    /// Phase 124.F.3 — session-level connectivity probe.
    ///
    /// Wire-level round-trip ("is the peer / agent / router
    /// reachable?") with `timeout_ms` budget. Returns
    /// `Result::success()` on reply, `ErrorCode::Timeout` on no
    /// reply, `ErrorCode::Unsupported` when the active backend
    /// can't probe. Mirrors micro-ROS's `rmw_uros_ping_agent`.
    ///
    /// Useful for reconnect-on-link-loss patterns — call
    /// periodically and tear down / re-open the executor on
    /// timeout.
    Result ping(int32_t timeout_ms) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_executor_ping(storage_, timeout_ms));
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
        // Phase 118.C: budget by wall-clock. Iteration-count budgeting
        // (`elapsed += timeout`) breaks when `nros_cpp_spin_once` returns
        // early on a signaled condvar — the loop collapses into
        // milliseconds. Same fix Future::wait() got in Phase 89.2.
        const uint64_t start_ns = nros_cpp_time_ns();
        const uint64_t budget_ns = static_cast<uint64_t>(duration_ms) * 1000000ULL;
        Result last = Result::success();
        while (true) {
            last = Result(nros_cpp_spin_once(storage_, poll_ms));
            if (!last.ok()) return last;
            const uint64_t now_ns = nros_cpp_time_ns();
            if (now_ns - start_ns >= budget_ns) break;
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
