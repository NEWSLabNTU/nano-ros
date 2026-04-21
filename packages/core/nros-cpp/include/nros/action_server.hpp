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

typedef int32_t (*nros_cpp_goal_callback_t)(const uint8_t goal_id[16], const uint8_t* data,
                                            size_t len, void* ctx);
typedef int32_t (*nros_cpp_cancel_callback_t)(const uint8_t goal_id[16], void* ctx);
typedef void (*nros_cpp_active_goal_visitor_t)(const uint8_t goal_id[16], int8_t status, void* ctx);

nros_cpp_ret_t nros_cpp_action_server_set_callbacks(void* handle, nros_cpp_goal_callback_t goal_cb,
                                                    nros_cpp_cancel_callback_t cancel_cb,
                                                    void* ctx);

nros_cpp_ret_t nros_cpp_action_server_publish_feedback(void* handle, void* executor_handle,
                                                       const uint8_t goal_id[16],
                                                       const uint8_t* feedback_buf,
                                                       size_t feedback_len);
nros_cpp_ret_t nros_cpp_action_server_complete_goal(void* handle, void* executor_handle,
                                                    const uint8_t goal_id[16],
                                                    const uint8_t* result_buf, size_t result_len);
nros_cpp_ret_t nros_cpp_action_server_for_each_active_goal(void* handle, void* executor_handle,
                                                           nros_cpp_active_goal_visitor_t visitor,
                                                           void* ctx);
nros_cpp_ret_t nros_cpp_action_server_destroy(void* storage);
nros_cpp_ret_t nros_cpp_action_server_relocate(void* old_storage, void* new_storage);
} // extern "C"

namespace nros {

/// Goal acceptance response returned from the user's goal callback.
enum class GoalResponse : int32_t {
    Reject = 0,
    AcceptAndExecute = 1,
    AcceptAndDefer = 2,
};

/// Cancel acceptance response returned from the user's cancel callback.
enum class CancelResponse : int32_t {
    Reject = 0,
    Accept = 1,
};

/// Mirror of `action_msgs/msg/GoalStatus` — lifecycle state reported by
/// `for_each_active_goal`.
enum class GoalStatus : int8_t {
    Unknown = 0,
    Accepted = 1,
    Executing = 2,
    Canceling = 3,
    Succeeded = 4,
    Canceled = 5,
    Aborted = 6,
};

/// Typed action server for a ROS 2 action.
///
/// Mirrors `rclcpp_action::Server<A>` with a callback-based API. The
/// action type `A` must provide nested `Goal`, `Result`, and `Feedback`
/// types with `TYPE_NAME`, `TYPE_HASH`, `SERIALIZED_SIZE_MAX`,
/// `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Usage:
/// ```cpp
/// using Fib = example_interfaces::action::Fibonacci;
/// nros::ActionServer<Fib> srv;
/// NROS_TRY(node.create_action_server(srv, "/fibonacci"));
///
/// srv.set_goal_callback(
///     [](const uint8_t[16], const Fib::Goal& g) {
///         if (g.order > 46) return nros::GoalResponse::Reject;
///         return nros::GoalResponse::AcceptAndExecute;
///     });
/// ```
///
/// Callbacks must be stateless (empty-capture lambdas or plain function
/// pointers). This is a freestanding C++14 library without `std::function`,
/// so per-instance closure storage is not available.
template <typename A> class ActionServer {
  public:
    using GoalType = typename A::Goal;
    using ResultType = typename A::Result;
    using FeedbackType = typename A::Feedback;

    /// User-facing typed goal callback signature.
    using TypedGoalFn = GoalResponse (*)(const uint8_t uuid[16], const GoalType& goal);
    /// User-facing typed cancel callback signature.
    using TypedCancelFn = CancelResponse (*)(const uint8_t uuid[16]);
    /// User-facing visitor signature for `for_each_active_goal`.
    using TypedVisitorFn = void (*)(const uint8_t uuid[16], GoalStatus status);

    /// Register a typed goal callback.
    ///
    /// `F` must be a stateless callable that decays to `TypedGoalFn`
    /// (empty-capture lambda or plain function pointer).
    /// F must be a stateless callable convertible to TypedGoalFn
    /// (empty-capture lambda or plain function pointer).
    template <typename F> Result set_goal_callback(F f) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        user_goal_fn_ = TypedGoalFn(f); // compile error if F is not convertible
        return install_callbacks();
    }

    /// Register a cancel callback.
    ///
    /// F must be a stateless callable convertible to TypedCancelFn.
    template <typename F> Result set_cancel_callback(F f) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        user_cancel_fn_ = TypedCancelFn(f); // compile error if F is not convertible
        return install_callbacks();
    }

    /// Publish feedback for an active goal.
    ///
    /// @param goal_id  16-byte goal UUID from the goal callback.
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
    Result complete_goal(const uint8_t goal_id[16], const ResultType& result) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);

        uint8_t buf[ResultType::SERIALIZED_SIZE_MAX];
        size_t len = 0;
        if (ResultType::ffi_serialize(&result, buf, sizeof(buf), &len) != 0) {
            return Result(ErrorCode::Error);
        }
        return Result(nros_cpp_action_server_complete_goal(storage_, executor_, goal_id, buf, len));
    }

    /// Iterate over every currently live goal and invoke `f(uuid, status)`.
    ///
    /// `F` must be a stateless callable convertible to
    /// `void (*)(const uint8_t uuid[16], GoalStatus status)`. The arena
    /// never stores the original goal CDR payload, so only identity +
    /// status are forwarded — if you need the goal bytes, stash them in
    /// a `{uuid → state}` table from inside `set_goal_callback`.
    /// F must be a stateless callable convertible to void(*)(const uint8_t[16], GoalStatus).
    template <typename F> Result for_each_active_goal(F f) {
        using Fn = void (*)(const uint8_t[16], GoalStatus);
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        user_visitor_fn_ = Fn(f); // compile error if F is not convertible

        auto trampoline = [](const uint8_t goal_id[16], int8_t status, void* ctx) {
            auto* self = static_cast<ActionServer*>(ctx);
            if (!self || self->user_visitor_fn_ == nullptr) return;
            self->user_visitor_fn_(goal_id, static_cast<GoalStatus>(status));
        };
        Result ret(
            nros_cpp_action_server_for_each_active_goal(storage_, executor_, +trampoline, this));
        user_visitor_fn_ = nullptr; // one-shot — don't leak the function pointer between calls
        return ret;
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

    // Move semantics (non-copyable). Relocation goes through the
    // Rust-side `nros_cpp_action_server_relocate` FFI (Phase 84.C1) and
    // then `install_callbacks()` re-registers the goal/cancel trampolines
    // with the new `this` as the arena callback context — this is the one
    // type in nros-cpp that registers its storage address externally.
    ActionServer(ActionServer&& other)
        : executor_(other.executor_), user_goal_fn_(other.user_goal_fn_),
          user_cancel_fn_(other.user_cancel_fn_), user_visitor_fn_(other.user_visitor_fn_),
          initialized_(other.initialized_) {
        if (other.initialized_) {
            nros_cpp_action_server_relocate(other.storage_, storage_);
            other.initialized_ = false;
            install_callbacks();
        }
    }

    ActionServer& operator=(ActionServer&& other) {
        if (this != &other) {
            if (initialized_) {
                nros_cpp_action_server_destroy(storage_);
            }
            executor_ = other.executor_;
            user_goal_fn_ = other.user_goal_fn_;
            user_cancel_fn_ = other.user_cancel_fn_;
            user_visitor_fn_ = other.user_visitor_fn_;
            initialized_ = other.initialized_;
            if (other.initialized_) {
                nros_cpp_action_server_relocate(other.storage_, storage_);
                other.initialized_ = false;
                install_callbacks();
            }
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized action server.
    /// Use `Node::create_action_server()` to initialize.
    ActionServer()
        : executor_(nullptr), user_goal_fn_(nullptr), user_cancel_fn_(nullptr),
          user_visitor_fn_(nullptr), initialized_(false) {}

  private:
    ActionServer(const ActionServer&) = delete;
    ActionServer& operator=(const ActionServer&) = delete;

    friend class Node;

    // ── C trampolines ───────────────────────────────────────────────
    //
    // `ctx` is a pointer to this `ActionServer<A>` instance, so the
    // trampoline reads the user's stored function pointer via the
    // instance's own fields — no shared mutable statics.

    static int32_t goal_trampoline(const uint8_t goal_id[16], const uint8_t* data, size_t len,
                                   void* ctx) {
        auto* self = static_cast<ActionServer*>(ctx);
        if (!self || self->user_goal_fn_ == nullptr) {
            return static_cast<int32_t>(GoalResponse::Reject);
        }
        GoalType g;
        if (GoalType::ffi_deserialize(data, len, &g) != 0) {
            return static_cast<int32_t>(GoalResponse::Reject);
        }
        return static_cast<int32_t>(self->user_goal_fn_(goal_id, g));
    }

    static int32_t cancel_trampoline(const uint8_t goal_id[16], void* ctx) {
        auto* self = static_cast<ActionServer*>(ctx);
        if (!self || self->user_cancel_fn_ == nullptr) {
            return static_cast<int32_t>(CancelResponse::Accept);
        }
        return static_cast<int32_t>(self->user_cancel_fn_(goal_id));
    }

    Result install_callbacks() {
        nros_cpp_goal_callback_t gcb = user_goal_fn_ ? &goal_trampoline : nullptr;
        nros_cpp_cancel_callback_t ccb = user_cancel_fn_ ? &cancel_trampoline : nullptr;
        return Result(nros_cpp_action_server_set_callbacks(storage_, gcb, ccb, this));
    }

    alignas(8) uint8_t storage_[NROS_CPP_ACTION_SERVER_STORAGE_SIZE];
    void* executor_; // Executor context needed for feedback/result operations
    TypedGoalFn user_goal_fn_;
    TypedCancelFn user_cancel_fn_;
    TypedVisitorFn user_visitor_fn_;
    bool initialized_;
};

} // namespace nros

#endif // NROS_CPP_ACTION_SERVER_HPP
