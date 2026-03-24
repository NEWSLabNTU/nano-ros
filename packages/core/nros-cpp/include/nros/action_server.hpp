// nros-cpp: Action server class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_ACTION_SERVER_HPP
#define NROS_CPP_ACTION_SERVER_HPP

#include <cstdint>
#include <cstddef>
#include <string.h>

#include "nros/config.hpp"
#include "nros/result.hpp"

// FFI declarations (create is declared in node.hpp with full type info)
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_action_server_try_recv_goal(void* handle, uint8_t* goal_buf, size_t buf_len,
                                                    size_t* goal_len, uint8_t goal_id_out[16]);
nros_cpp_ret_t nros_cpp_action_server_publish_feedback(void* handle, void* executor_handle,
                                                       const uint8_t goal_id[16],
                                                       const uint8_t* feedback_buf,
                                                       size_t feedback_len);
nros_cpp_ret_t nros_cpp_action_server_complete_goal(void* handle, void* executor_handle,
                                                    const uint8_t goal_id[16],
                                                    const uint8_t* result_buf, size_t result_len);
nros_cpp_ret_t nros_cpp_action_server_destroy(void* storage);
} // extern "C"

namespace nros {

/// Typed action server for a ROS 2 action.
///
/// Mirrors `rclcpp_action::Server<A>` (polling model). The action type `A` must provide
/// nested `Goal`, `Result`, and `Feedback` types with `TYPE_NAME`, `TYPE_HASH`,
/// `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Goals are auto-accepted during `spin_once()`. Use `try_recv_goal()` to poll
/// for accepted goal requests, then `publish_feedback()` and `complete_goal()`
/// to drive the action lifecycle.
///
/// Usage:
/// ```cpp
/// nros::ActionServer<example_interfaces::action::Fibonacci> srv;
/// NROS_TRY(node.create_action_server(srv, "/fibonacci"));
/// typename decltype(srv)::GoalType goal;
/// uint8_t goal_id[16];
/// if (srv.try_recv_goal(goal, goal_id)) {
///     typename decltype(srv)::FeedbackType fb;
///     fb.partial_sequence = ...;
///     srv.publish_feedback(goal_id, fb);
///     typename decltype(srv)::ResultType result;
///     result.sequence = ...;
///     srv.complete_goal(goal_id, result);
/// }
/// ```
template <typename A> class ActionServer {
  public:
    using GoalType = typename A::Goal;
    using ResultType = typename A::Result;
    using FeedbackType = typename A::Feedback;

    /// Try to receive a pending goal request (non-blocking).
    ///
    /// Goals are auto-accepted during `spin_once()`. This returns the next
    /// buffered goal.
    ///
    /// @param goal     Output goal struct (filled on success).
    /// @param goal_id  Output 16-byte goal UUID (filled on success).
    /// @return true if a goal was received and deserialized.
    bool try_recv_goal(GoalType& goal, uint8_t goal_id[16]) {
        if (!initialized_) return false;

        uint8_t buf[GoalType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        nros_cpp_ret_t ret =
            nros_cpp_action_server_try_recv_goal(storage_, buf, sizeof(buf), &len, goal_id);
        if (ret != 0 || len == 0) return false;
        if (GoalType::ffi_deserialize(buf, len, &goal) != 0) return false;
        return true;
    }

    /// Publish feedback for an active goal.
    ///
    /// @param goal_id  16-byte goal UUID from try_recv_goal().
    /// @param feedback Feedback to publish.
    /// @return Result indicating success or failure.
    Result publish_feedback(const uint8_t goal_id[16], const FeedbackType& feedback) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[FeedbackType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (FeedbackType::ffi_serialize(&feedback, buf, sizeof(buf), &len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(
            nros_cpp_action_server_publish_feedback(storage_, executor_, goal_id, buf, len));
    }

    /// Complete a goal with a result.
    ///
    /// @param goal_id  16-byte goal UUID from try_recv_goal().
    /// @param result   Result to send.
    /// @return Result indicating success or failure.
    Result complete_goal(const uint8_t goal_id[16], const ResultType& result) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[ResultType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (ResultType::ffi_serialize(&result, buf, sizeof(buf), &len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(nros_cpp_action_server_complete_goal(storage_, executor_, goal_id, buf, len));
    }

    /// Check if the action server is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases action server resources.
    ~ActionServer() {
        if (initialized_) {
            nros_cpp_action_server_destroy(storage_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    ActionServer(ActionServer&& other)
        : executor_(other.executor_), initialized_(other.initialized_) {
        if (other.initialized_) {
            memcpy(storage_, other.storage_, sizeof(storage_));
            other.initialized_ = false;
        }
    }

    ActionServer& operator=(ActionServer&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_action_server_destroy(storage_);
            }
            executor_ = other.executor_;
            initialized_ = other.initialized_;
            if (other.initialized_) {
                memcpy(storage_, other.storage_, sizeof(storage_));
                other.initialized_ = false;
            }
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized action server.
    /// Use `Node::create_action_server()` to initialize.
    ActionServer() : executor_(nullptr), initialized_(false) {}

  private:
    ActionServer(const ActionServer&) = delete;
    ActionServer& operator=(const ActionServer&) = delete;

    friend class Node;

    alignas(8) uint8_t storage_[NROS_CPP_ACTION_SERVER_STORAGE_SIZE];
    void* executor_; // Executor context needed for feedback/result operations
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_ACTION_SERVER_HPP
