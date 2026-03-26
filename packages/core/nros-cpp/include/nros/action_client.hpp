// nros-cpp: Action client class
// Freestanding C++ — no exceptions, no STL required

#ifndef NROS_CPP_ACTION_CLIENT_HPP
#define NROS_CPP_ACTION_CLIENT_HPP

#include <cstdint>
#include <cstddef>
#include <string.h>

#include "nros/config.hpp"
#include "nros/result.hpp"

// FFI declarations (create is declared in node.hpp with full type info)
extern "C" {
typedef int nros_cpp_ret_t;
nros_cpp_ret_t nros_cpp_action_client_send_goal(void* handle, const uint8_t* goal_buf,
                                                size_t goal_len, uint8_t goal_id_out[16]);
nros_cpp_ret_t nros_cpp_action_client_send_goal_async(void* handle, const uint8_t* goal_buf,
                                                      size_t goal_len, uint8_t goal_id_out[16]);
nros_cpp_ret_t nros_cpp_action_client_get_result(void* handle, void* executor_handle,
                                                 const uint8_t goal_id[16], uint8_t* result_buf,
                                                 size_t result_buf_len, size_t* result_len);
nros_cpp_ret_t nros_cpp_action_client_get_result_async(void* handle, const uint8_t goal_id[16]);
nros_cpp_ret_t nros_cpp_action_client_try_recv_feedback(void* handle, uint8_t* feedback_buf,
                                                        size_t buf_len, size_t* feedback_len);
nros_cpp_ret_t nros_cpp_action_client_destroy(void* storage);
} // extern "C"

namespace nros {

/// Typed action client for a ROS 2 action.
///
/// Mirrors `rclcpp_action::Client<A>`. The action type `A` must provide
/// nested `Goal`, `Result`, and `Feedback` types with `TYPE_NAME`, `TYPE_HASH`,
/// `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Usage:
/// ```cpp
/// nros::ActionClient<example_interfaces::action::Fibonacci> client;
/// NROS_TRY(node.create_action_client(client, "/fibonacci"));
/// typename decltype(client)::GoalType goal;
/// goal.order = 10;
/// uint8_t goal_id[16];
/// NROS_TRY(client.send_goal(goal, goal_id));
/// typename decltype(client)::ResultType result;
/// NROS_TRY(client.get_result(goal_id, result));
/// ```
template <typename A> class ActionClient {
  public:
    using GoalType = typename A::Goal;
    using ResultType = typename A::Result;
    using FeedbackType = typename A::Feedback;

    /// Send a goal and receive the generated goal UUID.
    ///
    /// @param goal     Goal to send.
    /// @param goal_id  Output 16-byte goal UUID (filled on success).
    /// @return Result indicating success or failure.
    Result send_goal(const GoalType& goal, uint8_t goal_id[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[GoalType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(nros_cpp_action_client_send_goal(storage_, buf, len, goal_id));
    }

    /// Get the result for a goal (blocking with timeout).
    ///
    /// Sends a get_result request and polls until a reply arrives or timeout.
    ///
    /// @param goal_id  16-byte goal UUID from send_goal().
    /// @param result   Output result struct (filled on success).
    /// @return Result indicating success, timeout, or failure.
    Result get_result(const uint8_t goal_id[16], ResultType& result) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[ResultType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        nros_cpp_ret_t ret =
            nros_cpp_action_client_get_result(storage_, executor_, goal_id, buf, sizeof(buf), &len);
        if (ret != 0) return Result(ret);

        if (ResultType::ffi_deserialize(buf, len, &result) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result::success();
    }

    /// Try to receive feedback (non-blocking).
    ///
    /// @param feedback Output feedback struct (filled on success).
    /// @return true if feedback was received and deserialized.
    bool try_recv_feedback(FeedbackType& feedback) {
        if (!initialized_) return false;

        uint8_t buf[FeedbackType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        nros_cpp_ret_t ret =
            nros_cpp_action_client_try_recv_feedback(storage_, buf, sizeof(buf), &len);
        if (ret != 0 || len == 0) return false;
        if (FeedbackType::ffi_deserialize(buf, len, &feedback) != 0) return false;
        return true;
    }

    // =================================================================
    // Async (non-blocking) API — callbacks invoked during spin_once()
    // =================================================================

    /// Options for async goal sending (mirrors rclcpp SendGoalOptions).
    ///
    /// Set callback pointers before calling send_goal_async(). Callbacks are
    /// invoked during spin_once() when the corresponding response arrives.
    /// All callbacks receive the context pointer for user state.
    struct SendGoalOptions {
        /// Called when the server accepts or rejects the goal.
        void (*goal_response)(bool accepted, const uint8_t goal_id[16], void* ctx);
        /// Called when feedback is received for the goal.
        void (*feedback)(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx);
        /// Called when the result is received.
        void (*result)(const uint8_t goal_id[16], int status, const uint8_t* data, size_t len,
                       void* ctx);
        /// User context pointer passed to all callbacks.
        void* context;

        SendGoalOptions() : goal_response(0), feedback(0), result(0), context(0) {}
    };

    /// Send a goal asynchronously (non-blocking).
    ///
    /// Returns immediately after sending the goal request. The goal UUID
    /// is filled on success. Responses arrive via callbacks registered
    /// with the executor (see SendGoalOptions and Node::create_action_client).
    ///
    /// @param goal     Goal to send.
    /// @param goal_id  Output 16-byte goal UUID (filled on success).
    /// @return Result indicating success or failure.
    Result send_goal_async(const GoalType& goal, uint8_t goal_id[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[GoalType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(nros_cpp_action_client_send_goal_async(storage_, buf, len, goal_id));
    }

    /// Request the result for a goal asynchronously (non-blocking).
    ///
    /// Returns immediately after sending the get_result request. The result
    /// arrives via the result callback during spin_once().
    ///
    /// @param goal_id  16-byte goal UUID from send_goal_async().
    /// @return Result indicating success or failure.
    Result get_result_async(const uint8_t goal_id[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_get_result_async(storage_, goal_id));
    }

    /// Check if the action client is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases action client resources.
    ~ActionClient() {
        if (initialized_) {
            nros_cpp_action_client_destroy(storage_);
            initialized_ = false;
        }
    }

    // Move semantics (non-copyable)
    ActionClient(ActionClient&& other)
        : executor_(other.executor_), initialized_(other.initialized_) {
        if (other.initialized_) {
            memcpy(storage_, other.storage_, sizeof(storage_));
            other.initialized_ = false;
        }
    }

    ActionClient& operator=(ActionClient&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_action_client_destroy(storage_);
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

    /// Default constructor — creates an uninitialized action client.
    /// Use `Node::create_action_client()` to initialize.
    ActionClient() : executor_(nullptr), initialized_(false) {}

  private:
    ActionClient(const ActionClient&) = delete;
    ActionClient& operator=(const ActionClient&) = delete;

    friend class Node;

    alignas(8) uint8_t storage_[NROS_CPP_ACTION_CLIENT_STORAGE_SIZE];
    void* executor_; // Executor context needed for get_result polling
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_ACTION_CLIENT_HPP
