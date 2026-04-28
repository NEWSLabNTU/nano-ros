// nros-cpp: Action client class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file action_client.hpp
 * @ingroup grp_action
 * @brief `nros::ActionClient<A>` — typed action client.
 */

#ifndef NROS_CPP_ACTION_CLIENT_HPP
#define NROS_CPP_ACTION_CLIENT_HPP

#include <cstdint>
#include <cstddef>
#include <string.h>

#include "nros/config.hpp"
#include "nros/result.hpp"
#include "nros/future.hpp"
#include "nros/stream.hpp"

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
nros_cpp_ret_t nros_cpp_action_client_try_recv_goal_response(void* handle, uint8_t* out_data,
                                                             size_t out_capacity, size_t* out_len);
nros_cpp_ret_t nros_cpp_action_client_try_recv_result(void* handle, uint8_t* out_data,
                                                      size_t out_capacity, size_t* out_len);
nros_cpp_ret_t nros_cpp_action_client_set_callbacks(
    void* handle, void (*goal_response)(bool accepted, const uint8_t goal_id[16], void* ctx),
    void (*feedback)(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx),
    void (*result)(const uint8_t goal_id[16], int status, const uint8_t* data, size_t len,
                   void* ctx),
    void* context);
nros_cpp_ret_t nros_cpp_action_client_poll(void* handle);
nros_cpp_ret_t nros_cpp_action_client_destroy(void* storage);
nros_cpp_ret_t nros_cpp_action_client_relocate(void* old_storage, void* new_storage);
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

    /// Goal acceptance response for the Future pattern.
    ///
    /// Returned by `send_goal_future()`. Contains the goal UUID and
    /// whether the server accepted the goal.
    struct GoalAccept {
        uint8_t goal_id[16];
        bool accepted;

        static const size_t SERIALIZED_SIZE_MAX = 32;
        static int ffi_deserialize(const uint8_t* data, size_t len, GoalAccept* out) {
            if (!out || len < 17) return -1;
            for (int i = 0; i < 16; ++i)
                out->goal_id[i] = data[i];
            out->accepted = data[16] != 0;
            return 0;
        }
    };

    /// Send a goal and receive the generated goal UUID (blocking).
    ///
    /// Internally spins the executor until the server accepts or rejects
    /// the goal (Phase 82 compliant -- drives the executor).
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
    /// Sends a get_result request and spins the executor until a reply
    /// arrives or timeout (Phase 82 compliant -- drives the executor).
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

    // =================================================================
    // Future-based API — non-blocking, polled via Future<T>
    // =================================================================

    /// Send a goal and return a Future for the acceptance response.
    ///
    /// Returns immediately after sending the goal request. Poll the
    /// returned future (or call `wait()`) to get the `GoalAccept` result.
    ///
    /// Usage:
    /// ```cpp
    /// auto fut = client.send_goal_future(goal);
    /// GoalAccept accept;
    /// NROS_TRY(fut.wait(executor.handle(), 5000, accept));
    /// if (accept.accepted) { /* use accept.goal_id */ }
    /// ```
    ///
    /// @param goal  Goal to send.
    /// @return Future that resolves to GoalAccept. Returns a consumed
    ///         (empty) future on serialization or send failure.
    Future<GoalAccept> send_goal_future(const GoalType& goal) {
        if (!initialized_) return Future<GoalAccept>();

        uint8_t buf[GoalType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0) {
            return Future<GoalAccept>();
        }

        uint8_t goal_id[16];
        nros_cpp_ret_t ret = nros_cpp_action_client_send_goal_async(storage_, buf, len, goal_id);
        if (ret != 0) return Future<GoalAccept>();

        return Future<GoalAccept>(storage_, &nros_cpp_action_client_try_recv_goal_response,
                                  0 // slot 0 (single outstanding goal request)
        );
    }

    /// Request a goal result and return a Future for the result.
    ///
    /// Sends the get_result request asynchronously and returns a Future
    /// that resolves when the result arrives. Poll the future (or call
    /// `wait()`) to retrieve the deserialized result.
    ///
    /// Usage:
    /// ```cpp
    /// auto fut = client.get_result_future(goal_id);
    /// ResultType result;
    /// NROS_TRY(fut.wait(executor.handle(), 10000, result));
    /// ```
    ///
    /// @param goal_id  16-byte goal UUID from send_goal() or GoalAccept.
    /// @return Future that resolves to ResultType. Returns a consumed
    ///         (empty) future on send failure.
    Future<ResultType> get_result_future(const uint8_t goal_id[16]) {
        if (!initialized_) return Future<ResultType>();

        nros_cpp_ret_t ret = nros_cpp_action_client_get_result_async(storage_, goal_id);
        if (ret != 0) return Future<ResultType>();

        return Future<ResultType>(storage_, &nros_cpp_action_client_try_recv_result,
                                  0 // slot 0 (single outstanding result request)
        );
    }

    /// Try to receive feedback (non-blocking).
    ///
    /// @param feedback Output feedback struct (filled on success).
    /// @return Result::success() if feedback was received and deserialized;
    ///         ErrorCode::TryAgain if no feedback is available right now;
    ///         ErrorCode::NotInitialized if the client is not initialized;
    ///         ErrorCode::Error if deserialization failed; otherwise the
    ///         FFI error code.
    Result try_recv_feedback(FeedbackType& feedback) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[FeedbackType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        nros_cpp_ret_t ret =
            nros_cpp_action_client_try_recv_feedback(storage_, buf, sizeof(buf), &len);
        if (ret != 0) return Result(ret);
        if (len == 0) return Result(ErrorCode::TryAgain);
        if (FeedbackType::ffi_deserialize(buf, len, &feedback) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result::success();
    }

    /// Get a reference to the action client's feedback stream.
    ///
    /// The stream yields `FeedbackType` values across all currently-active
    /// goals for this client — feedback is not goal-scoped at this layer.
    /// Callers that need per-goal separation should use the callback API
    /// (`set_callbacks(SendGoalOptions{ .feedback = … })`), which delivers
    /// `(goal_id, bytes, len, ctx)` via an executor-driven trampoline.
    ///
    /// Usage (blocking):
    /// ```cpp
    /// FeedbackType fb;
    /// NROS_TRY(client.feedback_stream().wait_next(executor.handle(), 500, fb));
    /// ```
    ///
    /// Usage (non-blocking):
    /// ```cpp
    /// FeedbackType fb;
    /// Result r = client.feedback_stream().try_next(fb);
    /// if (r.ok()) { ... }
    /// ```
    Stream<FeedbackType>& feedback_stream() {
        if (initialized_ && !feedback_stream_.is_valid()) {
            feedback_stream_.bind(storage_, &nros_cpp_action_client_try_recv_feedback);
        }
        return feedback_stream_;
    }

    const Stream<FeedbackType>& feedback_stream() const { return feedback_stream_; }

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
    /// arrives via the result callback during poll().
    ///
    /// @param goal_id  16-byte goal UUID from send_goal_async().
    /// @return Result indicating success or failure.
    Result get_result_async(const uint8_t goal_id[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_get_result_async(storage_, goal_id));
    }

    /// Register async callbacks for goal response, feedback, and result.
    ///
    /// @param options  Callback pointers and context.
    /// @return Result::success() on success, ErrorCode::NotInitialized
    ///         if the client is not initialized, or the FFI error code.
    Result set_callbacks(const SendGoalOptions& options) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_set_callbacks(
            storage_, options.goal_response, options.feedback, options.result, options.context));
    }

    /// Poll for pending async replies (non-blocking).
    ///
    /// Checks for goal acceptance, feedback, and result replies.
    /// Invokes the corresponding callbacks registered via set_callbacks().
    /// Call this in the spin loop after spin_once().
    void poll() {
        if (!initialized_) return;
        nros_cpp_action_client_poll(storage_);
    }

    /// Check if the action client is initialized and valid.
    bool is_valid() const { return initialized_; }

    /// Destructor — releases action client resources.
    ~ActionClient() {
        if (initialized_) {
            nros_cpp_action_client_destroy(storage_);
            initialized_ = false;
        }
        feedback_stream_ = Stream<FeedbackType>();
    }

    // Move semantics (non-copyable). Relocation goes through the
    // Rust-side `nros_cpp_action_client_relocate` FFI (Phase 84.C1).
    // The feedback stream is rebound to the new storage afterwards.
    ActionClient(ActionClient&& other)
        : executor_(other.executor_), initialized_(other.initialized_) {
        if (other.initialized_) {
            nros_cpp_action_client_relocate(other.storage_, storage_);
            other.initialized_ = false;
        }
        other.feedback_stream_ = Stream<FeedbackType>();
    }

    ActionClient& operator=(ActionClient&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_action_client_destroy(storage_);
                feedback_stream_ = Stream<FeedbackType>();
            }
            executor_ = other.executor_;
            initialized_ = other.initialized_;
            if (other.initialized_) {
                nros_cpp_action_client_relocate(other.storage_, storage_);
                other.initialized_ = false;
            }
            other.feedback_stream_ = Stream<FeedbackType>();
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
    void* executor_; // Stashed executor handle (Phase 82) for blocking helpers
    bool initialized_;
    Stream<FeedbackType> feedback_stream_;
    // Phase 87.6: action name buffer moved C++-side.
    char action_name_[256] = {};
};

} // namespace nros

// Phase 84.G8: out-of-line definition of Node::create_action_client<A>().
#include "nros/node.hpp"

namespace nros {

template <typename A>
Result Node::create_action_client(ActionClient<A>& out, const char* action_name, const QoS& qos) {
    if (!initialized_) return Result(ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos;
    ffi_qos.reliability = static_cast<nros_cpp_qos_reliability_t>(qos.reliability_raw());
    ffi_qos.durability = static_cast<nros_cpp_qos_durability_t>(qos.durability_raw());
    ffi_qos.history = static_cast<nros_cpp_qos_history_t>(qos.history_raw());
    ffi_qos.depth = qos.depth();
    nros_cpp_ret_t ret = nros_cpp_action_client_create(&handle_, action_name, A::TYPE_NAME,
                                                       A::Goal::TYPE_HASH, ffi_qos, out.storage_);
    if (ret == 0) {
        // Phase 87.6: action_name buffer lives C++-side now.
        size_t name_len = 0;
        while (action_name[name_len] != '\0' && name_len + 1 < sizeof(out.action_name_)) {
            out.action_name_[name_len] = action_name[name_len];
            ++name_len;
        }
        out.action_name_[name_len] = '\0';
        out.executor_ = executor_handle_;
        out.initialized_ = true;
    }
    return Result(ret);
}

} // namespace nros

#endif // NROS_CPP_ACTION_CLIENT_HPP
