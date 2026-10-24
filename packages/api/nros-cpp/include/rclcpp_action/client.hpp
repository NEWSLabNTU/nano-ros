// nros-cpp: Action client class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file action_client.hpp
 * @ingroup grp_action
 * @brief `rclcpp_action::Client<A>` — typed action client.
 */

#ifndef NROS_RCLCPP_ACTION_CLIENT_HPP
#define NROS_RCLCPP_ACTION_CLIENT_HPP

#include <cstdint>
#include <cstddef>
#include <string.h>

#include "rclcpp/detail/config.hpp"
#include "rclcpp/detail/entity_name.hpp" // phase-444 — the one entity-name copy
#include "rclcpp/logging.hpp" // phase-417 stage 3 — NROS_RCLCPP_REFUSE_* + rclcpp::detail::refuse
#include "rclcpp/owned.hpp"   // issue 2015 — `Client<A>::SharedPtr`
#include "rclcpp/result.hpp"
#include "rclcpp/size_bound.hpp" // rclcpp::rx_buffer_capacity<M> — the receive-buffer size
#include "rclcpp/future.hpp"
#include "rclcpp/stream.hpp"
// Issue 0796 — `CancelReturnCode` (the `action_msgs/srv/CancelGoal` RPC status
// `cancel_goal` resolves to) is declared beside its per-goal sibling
// `CancelResponse` in action_server.hpp, which is also where
// polling_action_server.hpp reaches for them.
#include "rclcpp_action/server.hpp"
#include "rclcpp_action/detail/upstream.hpp" // issue 2015 — hosted goal-handle plumbing

// Phase 118.D: FFI declarations sourced from cbindgen-generated
// `nros_cpp_ffi.h`. cbindgen renders Rust `*const [u8; 16]` /
// `*mut [u8; 16]` parameters as C++ pointer-to-array
// (`const uint8_t (*goal_id)[16]`); member-function bodies below
// take `goal_id` as a 16-byte array (decays to `uint8_t*`) and
// `reinterpret_cast` at the call site to bridge the two ABI
// equivalent shapes.
#include "rclcpp/detail/nros_cpp_ffi.h"

extern "C" {
typedef void (*nros_cpp_action_client_goal_response_callback_t)(bool accepted,
                                                                const uint8_t goal_id[16],
                                                                void* ctx);
typedef void (*nros_cpp_action_client_feedback_callback_t)(const uint8_t goal_id[16],
                                                           const uint8_t* data, size_t len,
                                                           void* ctx);
typedef void (*nros_cpp_action_client_result_callback_t)(const uint8_t goal_id[16], int32_t status,
                                                         const uint8_t* data, size_t len,
                                                         void* ctx);

nros_cpp_ret_t nros_cpp_action_client_set_callbacks(
    void* handle, nros_cpp_action_client_goal_response_callback_t goal_response,
    nros_cpp_action_client_feedback_callback_t feedback,
    nros_cpp_action_client_result_callback_t result, void* context);
} // extern "C"

/// `rclcpp::Node` is named by the friend declaration below and by the
/// out-of-line `Node::create_*` bodies further down. `rclcpp/node.hpp` (included
/// below, after the class, so a consumer pays only for the entities it uses)
/// has the definition; a qualified friend needs the name to EXIST first, which
/// an unqualified `friend class Node;` used to supply implicitly.
///
/// phase-427 W7 — declared in `rclcpp::`, which is where the definition lives.
namespace rclcpp {
class Node;
}

// ============================================================================
// `rclcpp_action::Client<A>` -- DEFINED here (RFC-0089: rclcpp_action:: is the home)
// ============================================================================
//
// phase-428: the definition moved from `nros::ActionClient<A>` to
// `rclcpp_action::Client<A>` (the `nros::` alias went in phase-483 W1). A RENAME
// as well as a move -- upstream's namespace already says "action", so the type
// is `Client`, not `ActionClient`. Note the two `Client`s do not collide:
// `rclcpp::Client<S>` is the SERVICE client, in a different namespace.
namespace rclcpp_action {

/// `rclcpp_action::ResultCode` — issue 2015, upstream's client-side terminal
/// code, value for value: `int8_t`, the `action_msgs/msg/GoalStatus` numbers
/// of the three terminal states (and UNKNOWN). The same numbers as
/// `GoalStatus`, so a status from the goal-id-keyed callbacks converts with a
/// cast.
enum class ResultCode : int8_t {
    UNKNOWN = 0,
    SUCCEEDED = 4,
    CANCELED = 5,
    ABORTED = 6,
};

template <typename A> class Client;

/// `rclcpp_action::ClientGoalHandle<A>` — issue 2015, upstream's client-side
/// per-goal handle.
///
/// On a HOSTED build (`NROS_CPP_STD`) it is what `async_send_goal`'s
/// `SendGoalOptions` callbacks receive, as upstream's `SharedPtr`
/// (`std::shared_ptr`): the goal id and status, and — nested — the
/// `WrappedResult` the result callback gets. It owns no futures and no
/// subscription (the client does; the handle is a heap token over the goal id).
/// A freestanding build has no allocator for it: `SharedPtr` and
/// `WrappedResult` are refused there, and the client's goal-id-keyed callbacks
/// (`set_callbacks`) are the form.
template <typename A> class ClientGoalHandle {
  public:
    using Feedback = typename A::Feedback;
    using Result = typename A::Result;
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    using SharedPtr = ::std::shared_ptr<ClientGoalHandle>;
    using ConstSharedPtr = ::std::shared_ptr<const ClientGoalHandle>;
    /// What a result callback receives: upstream's `WrappedResult`.
    struct WrappedResult {
        GoalUUID goal_id;
        ResultCode code;
        ::std::shared_ptr<Result> result;
    };
    using GoalResponseCallback = ::std::function<void(SharedPtr)>;
    using FeedbackCallback =
        ::std::function<void(SharedPtr, const ::std::shared_ptr<const Feedback>)>;
    using ResultCallback = ::std::function<void(const WrappedResult&)>;
#else
    using SharedPtr = detail::Refused<ClientGoalHandle>;
    using ConstSharedPtr = detail::Refused<ClientGoalHandle>;
    using WrappedResult = detail::Refused<ClientGoalHandle>;
#endif

    explicit ClientGoalHandle(const GoalUUID& id)
        : goal_id_(id), status_(static_cast<int8_t>(GoalStatus::Accepted)), feedback_aware_(false),
          result_aware_(false) {}

    /// The goal's id.
    const GoalUUID& get_goal_id() const { return goal_id_; }
    /// The last status this client saw for the goal (`action_msgs` numbers).
    int8_t get_status() const { return status_; }
    /// A feedback callback was given for this goal.
    bool is_feedback_aware() const { return feedback_aware_; }
    /// A result callback was given for this goal (its result was requested).
    bool is_result_aware() const { return result_aware_; }

  private:
    friend class Client<A>;
    GoalUUID goal_id_;
    int8_t status_;
    bool feedback_aware_;
    bool result_aware_;
};

/// Typed action client for a ROS 2 action.
///
/// Mirrors upstream's `rclcpp_action::Client<A>`. The action type `A` must provide
/// nested `Goal`, `Result`, and `Feedback` types with `TYPE_NAME`, `TYPE_HASH`,
/// `SERIALIZED_SIZE_MAX`, `ffi_serialize()`, and `ffi_deserialize()`.
///
/// Usage:
/// ```cpp
/// rclcpp_action::Client<example_interfaces::action::Fibonacci> client;
/// RCLCPP_TRY(node.create_action_client(client, "/fibonacci"));
/// typename decltype(client)::GoalType goal;
/// goal.order = 10;
/// uint8_t goal_id[16];
/// RCLCPP_TRY(client.send_goal(goal, goal_id));
/// typename decltype(client)::ResultType result;
/// RCLCPP_TRY(client.get_result(goal_id, result));
/// ```
template <typename A> class Client {
  public:
    using GoalType = typename A::Goal;
    using ResultType = typename A::Result;
    using FeedbackType = typename A::Feedback;

    /// Issue 2015 — upstream's `Client<A>::SharedPtr`. Not a `std::shared_ptr`:
    /// `rclcpp::Owned<Client>` holds the client BY VALUE (no allocator), which
    /// is what `rclcpp_action::create_client` returns.
    using SharedPtr = ::rclcpp::Owned<Client>;
    /// Upstream's goal handle for this action (issue 2015).
    using GoalHandle = ::rclcpp_action::ClientGoalHandle<A>;
    /// Upstream's result wrapper (hosted; refused on a freestanding build).
    using WrappedResult = typename GoalHandle::WrappedResult;

  private:
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    using FeedbackPtr = ::std::shared_ptr<const FeedbackType>;
#else
    using FeedbackPtr = detail::Refused<FeedbackType>;
#endif

  public:
    /// Goal acceptance response for the Future pattern.
    ///
    /// Returned by `send_goal_future()`. Contains the goal UUID and
    /// whether the server accepted the goal.
    struct GoalAccept {
        /// Kept a raw array: it is written through the FFI decoder below, and
        /// in-tree callers pass it straight on to the raw-array overloads.
        /// `GoalUUID(accept.goal_id)` is the one-token lift (phase-417 W4.b).
        uint8_t goal_id[16];
        bool accepted;

        // Receive-buffer size for `Future<GoalAccept>`; the payload is 17 bytes
        // (16-byte goal_id + 1 accepted byte), rounded up.
        static const size_t SERIALIZED_SIZE_MAX = 32;

        // Issue 0329 — the 17-byte wire layout is owned Rust-side
        // (`nros_cpp_action_goal_accept_decode`, beside its producer); the header
        // no longer hand-decodes it, so the two cannot drift.
        static int ffi_deserialize(const uint8_t* data, size_t len, GoalAccept* out) {
            if (!out) return -1;
            return nros_cpp_action_goal_accept_decode(data, len, &out->goal_id, &out->accepted) ==
                           NROS_CPP_RET_OK
                       ? 0
                       : -1;
        }
    };

    /// phase-338 W8 — block until the action server is discoverable.
    ///
    /// Mirrors `rclcpp_action::Client::wait_for_action_server`. Probes the
    /// `send_goal` queryable, which is the load-bearing entity for the first
    /// `send_goal()`. Prefer this over retrying `send_goal()` on timeout: it
    /// waits for the real condition and re-probes, so a server that comes up
    /// after the wait starts is still seen.
    ///
    /// Spins the executor while probing — not for use inside a callback.
    /// Returns `Unsupported` at once when the backend cannot know whether a
    /// server is up (XRCE — issue 1686); a caller that may send blind proceeds.
    ///
    /// **The budget is REQUIRED** — phase-417 stage 3, the same defect and the
    /// same fix as `Client::wait_for_service`; they were found together and are
    /// corrected together, under one message. Upstream's
    /// `rclcpp_action::Client::wait_for_action_server(timeout = -1)` waits
    /// forever; this used to substitute 5000 ms for an argument-free call,
    /// silently. RFC-0021 is why the unbounded form does not exist here, and
    /// `NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT` is what a ported caller now reads.
    ///
    /// Takes `uint32_t` MILLISECONDS, not a `std::chrono::duration`, and returns
    /// `Result` (timeout vs. not-initialised by code), not `bool`.
    ::rclcpp::Result wait_for_action_server(uint32_t timeout_ms) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
        return ::rclcpp::Result(
            nros_cpp_action_client_wait_for_action_server(storage_, timeout_ms));
    }

    /// **REFUSED** — `wait_for_action_server()` with no budget.
    template <typename T = void>::rclcpp::Result wait_for_action_server() {
        static_assert(::rclcpp::detail::refuse<T>::value, NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT);
        return ::rclcpp::Result(::rclcpp::ErrorCode::Unsupported);
    }

#ifdef NROS_CPP_HAS_STD_CHRONO // hosted-family: chrono-interop
    /// `wait_for_action_server(std::chrono::duration)` — phase-488, upstream's argument type.
    /// The same wait as the millisecond form, with the budget rounded UP and
    /// clamped to `UINT32_MAX` ms; a NEGATIVE duration (upstream's "forever")
    /// is refused at the call. Still returns `Result`, whose explicit
    /// `operator bool` keeps `while (!client->wait_for_action_server(1s))` compiling.
    template <typename Rep, typename Period>
    ::rclcpp::Result wait_for_action_server(::std::chrono::duration<Rep, Period> timeout) {
        const int64_t ms = ::rclcpp::detail::chrono_wait_budget_ms(timeout);
        if (ms < 0) {
            NROS_RCLCPP_SAY_REFUSED(NROS_RCLCPP_REFUSE_UNBOUNDED_WAIT_RUNTIME);
            return ::rclcpp::Result(::rclcpp::ErrorCode::Unsupported);
        }
        return wait_for_action_server(static_cast<uint32_t>(ms));
    }
#endif // NROS_CPP_HAS_STD_CHRONO

    /// Is an action server for this action visible right now? — phase-417 W4.b.
    ///
    /// `rclcpp_action::ClientBase::action_server_is_ready`, the NON-BLOCKING
    /// half of @ref wait_for_action_server. C has had
    /// `rclc_action_client_action_server_is_ready` and Rust
    /// `ActionClient::action_server_is_ready` since phase-338; C++ had neither,
    /// so the only way to ask here was to block (ledger row
    /// `cpp:Client::action_server_is_ready`, a C++-only gap rather than a
    /// decision).
    ///
    /// Spins nothing, so unlike @ref wait_for_action_server it is legal inside
    /// a callback. Its answer is a SNAPSHOT: `false` means "not visible yet",
    /// never "will not appear".
    bool action_server_is_ready() const {
        if (!initialized_) return false;
        return nros_cpp_action_client_action_server_is_ready(
            const_cast<void*>(static_cast<const void*>(storage_)));
    }

    /// Read back the action name this client was created on — phase-417 W4.b.
    ///
    /// The client half of "four surfaces, one accessor" (ledger row
    /// `c:action_client_get_action_name`); see
    /// `rclcpp_action::Server<A>::get_action_name` for why the 256-byte copy is
    /// where it is. Returns `""` (never NULL) on an uninitialised client.
    const char* get_action_name() const { return initialized_ ? action_name_ : ""; }

    /// Send a goal and receive the generated goal UUID (blocking).
    ///
    /// Internally spins the executor until the server accepts or rejects
    /// the goal (Phase 82 compliant -- drives the executor).
    ///
    /// Upstream's `async_send_goal` returns a `std::shared_future` of a goal handle.
    /// This one blocks and reports through `Result` plus an out-param goal id: no
    /// future and no goal handle (RFC-0021: no async runtime, no allocator). The
    /// future-shaped spelling is `send_goal_future`, over `rclcpp::Future`.
    ///
    /// @param goal     Goal to send.
    /// @param goal_id  Output 16-byte goal UUID (filled on success).
    /// @return Result indicating success or failure.
    ::rclcpp::Result send_goal(const GoalType& goal, uint8_t goal_id[16]) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);

        uint8_t buf[::rclcpp::detail::buffer_bounds<GoalType>::tx];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0) {
            return ::rclcpp::Result(::rclcpp::ErrorCode::Error);
        }
        return ::rclcpp::Result(nros_cpp_action_client_send_goal(
            storage_, buf, len, reinterpret_cast<uint8_t(*)[16]>(goal_id)));
    }

    /// @ref send_goal writing the generated id into a `GoalUUID` value
    /// (phase-417 W4.b) — the shape you can put in a map.
    ::rclcpp::Result send_goal(const GoalType& goal, ::rclcpp_action::GoalUUID& goal_id) {
        return send_goal(goal, goal_id.data());
    }

    /// Get the result for a goal (blocking with timeout).
    ///
    /// Sends a get_result request and spins the executor until a reply
    /// arrives or timeout (Phase 82 compliant -- drives the executor).
    ///
    /// @param goal_id  16-byte goal UUID from send_goal().
    /// @param result   Output result struct (filled on success).
    /// @return Result indicating success, timeout, or failure.
    ::rclcpp::Result get_result(const uint8_t goal_id[16], ResultType& result) {
        return get_result_sized<::rclcpp::rx_buffer_capacity<ResultType>::value>(goal_id, result);
    }

    /// @ref get_result taking a `GoalUUID` value (phase-417 W4.b).
    ::rclcpp::Result get_result(const ::rclcpp_action::GoalUUID& goal_id, ResultType& result) {
        return get_result_sized<::rclcpp::rx_buffer_capacity<ResultType>::value>(goal_id.data(),
                                                                                 result);
    }

    /// @ref get_result with the receive buffer sized by the CALLER.
    /// See @ref Subscription::try_recv_sized (issue 0964).
    template <size_t Cap>
    ::rclcpp::Result get_result_sized(const uint8_t goal_id[16], ResultType& result) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);

        uint8_t buf[Cap];
        size_t len = 0;
        nros_cpp_ret_t ret = nros_cpp_action_client_get_result(
            storage_, executor_, reinterpret_cast<const uint8_t(*)[16]>(goal_id), buf, sizeof(buf),
            &len);
        if (ret != 0) return ::rclcpp::Result(ret);

        if (ResultType::ffi_deserialize(buf, len, &result) != 0) {
            return ::rclcpp::Result(::rclcpp::ErrorCode::Error);
        }
        return ::rclcpp::Result::success();
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
    /// RCLCPP_TRY(fut.wait(executor.handle(), 5000, accept));
    /// if (accept.accepted) { /* use accept.goal_id */ }
    /// ```
    ///
    /// The future is KEYED BY ITS GOAL (issue 2011): any number of goals may be
    /// in flight, each future receives only its own goal's acceptance, and the
    /// executor dispatching that reply during `spin_once` hands it to the
    /// future instead of racing it. Single-threaded, like the service futures:
    /// do not spin this client's executor on another thread while one is in
    /// flight.
    ///
    /// @param goal  Goal to send.
    /// @return Future that resolves to GoalAccept. Returns a consumed
    ///         (empty) future (`valid()` false) on serialization or send
    ///         failure -- including a full table: at most
    ///         `MAX_CONCURRENT_GOALS` acceptances may be pending per client
    ///         (the Rust core refuses the send before it reaches the wire).
    ::rclcpp::Future<GoalAccept> send_goal_future(const GoalType& goal) {
        if (!initialized_) return ::rclcpp::Future<GoalAccept>();

        uint8_t buf[::rclcpp::detail::buffer_bounds<GoalType>::tx];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0) {
            return ::rclcpp::Future<GoalAccept>();
        }

        uint8_t goal_id[16];
        nros_cpp_ret_t ret = nros_cpp_action_client_send_goal_async(
            storage_, buf, len, reinterpret_cast<uint8_t(*)[16]>(goal_id));
        if (ret != 0) return ::rclcpp::Future<GoalAccept>();

        // Issue 2011 -- keyed by THIS goal on this client's acceptance list:
        // the acceptance reaches it whether the executor dispatched it during
        // `spin_once` (through `on_goal_response`) or the future's own poll
        // took it (`nros_cpp_action_client_poll`, same callbacks).
        return ::rclcpp::Future<GoalAccept>(storage_, &nros_cpp_action_client_poll, goal_id,
                                            &accepts_,
                                            executor_ // driven by `Future::wait_for` (issue 1996)
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
    /// RCLCPP_TRY(fut.wait(executor.handle(), 10000, result));
    /// ```
    ///
    /// Keyed by `goal_id` (issue 2011): the future receives that goal's result
    /// and no other, whichever client or future polls first. A result for a
    /// goal no future waits on -- including one whose future was cancelled or
    /// destroyed first -- is dropped (the client's result callback still sees
    /// it). Several futures for one goal each receive its result, one reply
    /// apiece.
    ///
    /// @param goal_id  16-byte goal UUID from send_goal() or GoalAccept.
    /// @return Future that resolves to ResultType. Returns a consumed
    ///         (empty) future on send failure, including a full table (at most
    ///         `MAX_CONCURRENT_GOALS` get_result requests pending per client).
    ::rclcpp::Future<ResultType> get_result_future(const uint8_t goal_id[16]) {
        return get_result_future_sized<::rclcpp::rx_buffer_capacity<ResultType>::value>(goal_id);
    }

    /// @ref get_result_future taking a `GoalUUID` value (phase-417 W4.b).
    ::rclcpp::Future<ResultType> get_result_future(const ::rclcpp_action::GoalUUID& goal_id) {
        return get_result_future_sized<::rclcpp::rx_buffer_capacity<ResultType>::value>(
            goal_id.data());
    }

    /// @ref get_result_future with the RESULT buffer sized by the caller.
    ///
    /// A `Future<T>` holds its receive buffer as a member, so the capacity is a
    /// class template argument: this returns `Future<ResultType, Cap>`
    /// (issue 0964).
    template <size_t Cap>
    ::rclcpp::Future<ResultType, Cap> get_result_future_sized(const uint8_t goal_id[16]) {
        using Fut = ::rclcpp::Future<ResultType, Cap>;
        if (!initialized_) return Fut();

        nros_cpp_ret_t ret = nros_cpp_action_client_get_result_async(
            storage_, reinterpret_cast<const uint8_t(*)[16]>(goal_id));
        if (ret != 0) return Fut();

        // Issue 2011 -- keyed by THIS goal on this client's result list; see
        // `send_goal_future`.
        return Fut(storage_, &nros_cpp_action_client_poll, goal_id, &results_,
                   executor_ // driven by `Future::wait_for` (issue 1996)
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
    ::rclcpp::Result try_recv_feedback(FeedbackType& feedback) {
        return try_recv_feedback_sized<::rclcpp::rx_buffer_capacity<FeedbackType>::value>(feedback);
    }

    /// @ref try_recv_feedback with the receive buffer sized by the CALLER.
    /// See @ref Subscription::try_recv_sized (issue 0964).
    template <size_t Cap>::rclcpp::Result try_recv_feedback_sized(FeedbackType& feedback) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);

        uint8_t buf[Cap];
        size_t len = 0;
        nros_cpp_ret_t ret = take_feedback(this, buf, sizeof(buf), &len);
        if (ret != 0) return ::rclcpp::Result(ret);
        if (len == 0) return ::rclcpp::Result(::rclcpp::ErrorCode::TryAgain);
        if (FeedbackType::ffi_deserialize(buf, len, &feedback) != 0) {
            return ::rclcpp::Result(::rclcpp::ErrorCode::Error);
        }
        return ::rclcpp::Result::success();
    }

    /// Get a reference to the action client's feedback stream.
    ///
    /// The stream yields `FeedbackType` values across all currently-active
    /// goals for this client — feedback is not goal-scoped at this layer. It
    /// holds the LATEST feedback per client (issue 2011: it used to be one slot
    /// for every action client in the process); an unread one is overwritten.
    /// Callers that need per-goal separation should use the callback API
    /// (`set_callbacks(SendGoalOptions{ .feedback = … })`), which delivers
    /// `(goal_id, bytes, len, ctx)` via an executor-driven trampoline.
    ///
    /// Usage (blocking):
    /// ```cpp
    /// FeedbackType fb;
    /// RCLCPP_TRY(client.feedback_stream().wait_next(executor.handle(), 500, fb));
    /// ```
    ///
    /// Usage (non-blocking):
    /// ```cpp
    /// FeedbackType fb;
    /// Result r = client.feedback_stream().try_next(fb);
    /// if (r.ok()) { ... }
    /// ```
    ::rclcpp::Stream<FeedbackType>& feedback_stream() {
        if (initialized_ && !feedback_stream_.is_valid()) {
            feedback_stream_.bind(this, &Client::take_feedback);
        }
        return feedback_stream_;
    }

    const ::rclcpp::Stream<FeedbackType>& feedback_stream() const { return feedback_stream_; }

    // =================================================================
    // Async (non-blocking) API — callbacks invoked during spin_once()
    // =================================================================

    /// Options for async goal sending (mirrors rclcpp SendGoalOptions).
    ///
    /// Installed on the CLIENT with set_callbacks(), not passed per goal.
    /// Callbacks are invoked during spin_once() when the corresponding
    /// response arrives.
    /// All callbacks receive the context pointer for user state.
    ///
    /// Issue 2015 — on a HOSTED build it ALSO carries upstream's three
    /// per-goal fields, `goal_response_callback` / `feedback_callback` /
    /// `result_callback`, taking upstream's shapes (`GoalHandle::SharedPtr`,
    /// `std::shared_ptr<const Feedback>`, `const WrappedResult&`). Those are
    /// read by `async_send_goal`, per goal, as upstream does; the raw pointers
    /// are read by `set_callbacks`, per client. A freestanding build refuses an
    /// assignment to the upstream fields.
    struct SendGoalOptions {
        /// Called when the server accepts or rejects the goal.
        void (*goal_response)(bool accepted, const uint8_t goal_id[16], void* ctx);
        /// Called when feedback is received for the goal.
        void (*feedback)(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx);
        /// Called when the result is received.
        void (*result)(const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len,
                       void* ctx);
        /// User context pointer passed to all callbacks.
        void* context;

        /// Upstream's: the goal was accepted (a handle) or rejected (null).
        detail::UpstreamCallback<void(typename GoalHandle::SharedPtr)> goal_response_callback;
        /// Upstream's: feedback for the goal.
        detail::UpstreamCallback<void(typename GoalHandle::SharedPtr, FeedbackPtr)>
            feedback_callback;
        /// Upstream's: the goal's result. Giving one requests the result once
        /// the goal is accepted, as upstream does.
        detail::UpstreamCallback<void(const WrappedResult&)> result_callback;

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
    ::rclcpp::Result send_goal_async(const GoalType& goal, uint8_t goal_id[16]) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);

        uint8_t buf[::rclcpp::detail::buffer_bounds<GoalType>::tx];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0) {
            return ::rclcpp::Result(::rclcpp::ErrorCode::Error);
        }
        return ::rclcpp::Result(nros_cpp_action_client_send_goal_async(
            storage_, buf, len, reinterpret_cast<uint8_t(*)[16]>(goal_id)));
    }

    /// @ref send_goal_async writing the generated id into a `GoalUUID` value
    /// (phase-417 W4.b).
    ::rclcpp::Result send_goal_async(const GoalType& goal, ::rclcpp_action::GoalUUID& goal_id) {
        return send_goal_async(goal, goal_id.data());
    }

#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    /// `async_send_goal(goal, options)` — issue 2015, upstream's per-goal send.
    ///
    /// Sends the goal and binds `options`' upstream fields to THAT goal:
    /// `goal_response_callback` gets a `GoalHandle::SharedPtr` (null when the
    /// server rejected it), `feedback_callback` each feedback as a
    /// `std::shared_ptr<const Feedback>`, and `result_callback` the
    /// `WrappedResult` — giving one requests the result once the goal is
    /// accepted, as upstream does. Every callback runs from the executor's
    /// dispatch during `spin*`, beside the client-wide ones of
    /// `set_callbacks`.
    ///
    /// Upstream returns a `std::shared_future<GoalHandle::SharedPtr>`; this
    /// returns the goal-keyed `rclcpp::Future<GoalAccept>` that
    /// `send_goal_future` does (RFC-0021: no blocking future), so
    /// `auto f = client->async_send_goal(goal, opts);` compiles and a caller
    /// that waits on it gets the acceptance. An empty future (`valid()` false)
    /// means the send failed and no callback will run.
    ::rclcpp::Future<GoalAccept>
    async_send_goal(const GoalType& goal, const SendGoalOptions& options = SendGoalOptions()) {
        uint8_t goal_id[16];
        if (!send_goal_async(goal, goal_id).ok()) return ::rclcpp::Future<GoalAccept>();
        UpstreamState* st = upstream_.template as<UpstreamState>();
        if (st == nullptr) {
            st = new UpstreamState();
            upstream_.reset(st, [](void* p) { delete static_cast<UpstreamState*>(p); });
        }
        st->goals.push_back(UpstreamGoal{GoalUUID(goal_id), options, nullptr});
        return ::rclcpp::Future<GoalAccept>(storage_, &nros_cpp_action_client_poll, goal_id,
                                            &accepts_, executor_);
    }
#else
    /// **REFUSED** on a freestanding build — issue 2015.
    template <typename T = void>
    ::rclcpp::Future<GoalAccept> async_send_goal(const GoalType&,
                                                 const SendGoalOptions& = SendGoalOptions()) {
        static_assert(::rclcpp::detail::refuse<T>::value, NROS_RCLCPP_REFUSE_ACTION_GOAL_HANDLE);
        return ::rclcpp::Future<GoalAccept>();
    }
#endif

    /// Cancel a goal (non-blocking) — issue 0796.
    ///
    /// Sends the `action_msgs/srv/CancelGoal` request and returns; read the
    /// RPC outcome with `try_recv_cancel_response()` once the executor has
    /// spun. Mirrors C's `rclc_action_cancel_goal` and Rust's
    /// `ActionClient::cancel_goal` — rclcpp_action's `async_cancel_goal`
    /// returns a future, which RFC-0021 has no runtime to await.
    ///
    /// Until this existed, a C++ application written against the CALLBACK
    /// tier could start a goal and had no way to stop it: cancel was only on
    /// the L1 `PollingActionClient::send_cancel_request`.
    ///
    /// @param goal_id  16-byte goal UUID from send_goal() / send_goal_async().
    /// @return Result indicating the request was sent.
    ::rclcpp::Result cancel_goal(const uint8_t goal_id[16]) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
        return ::rclcpp::Result(nros_cpp_action_client_cancel_goal(
            storage_, reinterpret_cast<const uint8_t(*)[16]>(goal_id)));
    }

    /// @ref cancel_goal taking a `GoalUUID` value (phase-417 W4.b).
    ::rclcpp::Result cancel_goal(const ::rclcpp_action::GoalUUID& goal_id) {
        return cancel_goal(goal_id.data());
    }

    /// Try to read the reply to a `cancel_goal()` (non-blocking).
    ///
    /// @param out  Receives the RPC return code on success.
    /// @return Result::success() when a reply was consumed;
    ///         ErrorCode::TryAgain when none has arrived yet.
    ::rclcpp::Result try_recv_cancel_response(::rclcpp_action::CancelReturnCode& out) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
        int8_t code = 0;
        nros_cpp_ret_t ret = nros_cpp_action_client_try_recv_cancel_response(storage_, &code);
        if (ret != 0) return ::rclcpp::Result(ret);
        out = static_cast<::rclcpp_action::CancelReturnCode>(code);
        return ::rclcpp::Result::success();
    }

    /// Request the result for a goal asynchronously (non-blocking).
    ///
    /// Returns immediately after sending the get_result request. The result
    /// arrives via the result callback during poll().
    ///
    /// @param goal_id  16-byte goal UUID from send_goal_async().
    /// @return Result indicating success or failure.
    ::rclcpp::Result get_result_async(const uint8_t goal_id[16]) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
        return ::rclcpp::Result(nros_cpp_action_client_get_result_async(
            storage_, reinterpret_cast<const uint8_t(*)[16]>(goal_id)));
    }

    /// @ref get_result_async taking a `GoalUUID` value (phase-417 W4.b).
    ::rclcpp::Result get_result_async(const ::rclcpp_action::GoalUUID& goal_id) {
        return get_result_async(goal_id.data());
    }

    /// Register async callbacks for goal response, feedback, and result.
    ///
    /// Binds the callbacks to the CLIENT, not to one goal: rclcpp passes a
    /// `SendGoalOptions` to each `async_send_goal`, while here they are installed
    /// once and serve every goal this client sends (RFC-0041: callbacks bind to
    /// the entity). The pointers are read when a reply is DISPATCHED, so install
    /// them before the spin that delivers the reply — before or after
    /// `send_goal_async` both work.
    ///
    /// @param options  Callback pointers and context.
    /// @return Result::success() on success, ErrorCode::NotInitialized
    ///         if the client is not initialized, or the FFI error code.
    ::rclcpp::Result set_callbacks(const SendGoalOptions& options) {
        if (!initialized_) return ::rclcpp::Result(::rclcpp::ErrorCode::NotInitialized);
        // Issue 2011 -- the client keeps its OWN callbacks installed (they feed
        // the goal-keyed futures and the feedback slot) and forwards to these.
        user_ = options;
        return install_dispatch();
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

    /// Destructor — RELEASES the arena entry (issue 1496).
    ///
    /// The `ActionClientCore` with its three service clients and its feedback
    /// subscription lives in an `ActionClientRawArenaEntry` in the executor
    /// arena, whose callback `context` is `storage_`.
    /// `nros_cpp_action_client_destroy` (which needs no extra argument:
    /// `CppActionClient` keeps its own executor pointer and entry index) drops
    /// that entry in place — the entities are destroyed, so no late reply can
    /// read this storage — frees its callback slot and returns its arena bytes
    /// for reuse, so a create/destroy loop does not exhaust the executor.
    ///
    /// Issue 2011 -- a future still pending on this client becomes invalid
    /// (`valid()` false) instead of pointing at freed storage.
    ~Client() {
        accepts_.orphan_all();
        results_.orphan_all();
        if (initialized_) {
            nros_cpp_action_client_destroy(storage_);
            initialized_ = false;
        }
        feedback_stream_ = ::rclcpp::Stream<FeedbackType>();
    }

    // Move semantics (non-copyable). Relocation goes through the
    // `nros_cpp_action_client_relocate` runtime call (Phase 84.C1), which also
    // RE-POINTS the arena entry's callback `context` at the new `storage_`
    // (issue 1496 — before that, a moved-from temporary left the entry naming
    // dead bytes). The feedback stream is rebound to the new storage afterwards.
    //
    // Issue 2011 -- the pending futures and the client's own callbacks follow
    // it too: both lists move (each future's storage pointer re-targeted) and
    // the callbacks are re-installed with the new address as their context.
    Client(Client&& other)
        : executor_(other.executor_), initialized_(other.initialized_), action_name_{} {
        ::memcpy(action_name_, other.action_name_, sizeof(action_name_));
        adopt(other);
    }

    Client& operator=(Client&& other) {
        if (this != &other) {
            accepts_.orphan_all();
            results_.orphan_all();
            if (initialized_) {
                nros_cpp_action_client_destroy(storage_);
                feedback_stream_ = ::rclcpp::Stream<FeedbackType>();
            }
            executor_ = other.executor_;
            initialized_ = other.initialized_;
            ::memcpy(action_name_, other.action_name_, sizeof(action_name_));
            adopt(other);
        }
        return *this;
    }

    /// Default constructor — creates an uninitialized action client.
    /// Use `Node::create_action_client()` to initialize.
    Client() : executor_(nullptr), initialized_(false), action_name_{} {}

  private:
    Client(const Client&) = delete;
    Client& operator=(const Client&) = delete;

    friend class ::rclcpp::Node;

    /// Issue 2011 -- install this client's own callbacks (context = `this`).
    ::rclcpp::Result install_dispatch() {
        return ::rclcpp::Result(nros_cpp_action_client_set_callbacks(
            storage_, &Client::on_goal_response, &Client::on_feedback, &Client::on_result, this));
    }

    /// Take over `other`'s storage, pending futures, callbacks and feedback.
    void adopt(Client& other) {
        user_ = other.user_;
        upstream_ = static_cast<detail::HostedBox&&>(other.upstream_);
        feedback_len_ = other.feedback_len_;
        feedback_ready_ = other.feedback_ready_;
        feedback_truncated_ = other.feedback_truncated_;
        for (size_t i = 0; i < feedback_len_; ++i)
            feedback_buf_[i] = other.feedback_buf_[i];
        other.feedback_ready_ = false;
        if (other.initialized_) {
            nros_cpp_action_client_relocate(other.storage_, storage_);
            other.accepts_.move_to(accepts_, storage_);
            other.results_.move_to(results_, storage_);
            other.initialized_ = false;
            (void)install_dispatch();
        }
        other.feedback_stream_ = ::rclcpp::Stream<FeedbackType>();
    }

    // Issue 2011 -- the three callbacks every `Client<A>` keeps installed. The
    // executor's dispatch (and `nros_cpp_action_client_poll`) hands each reply
    // here, attributed to the goal its request named (issue 2009); it goes to
    // the future waiting on THAT goal on THIS client, if any -- dropped
    // otherwise, never handed to another goal -- and then to the user's
    // callback (`set_callbacks`).
    static void on_goal_response(bool accepted, const uint8_t goal_id[16], void* ctx) {
        Client* self = static_cast<Client*>(ctx);
        ::rclcpp::detail::PendingLink* owner = self->accepts_.resolve_goal(goal_id);
        if (owner != nullptr) {
            // The 17-byte layout `GoalAccept::ffi_deserialize` decodes
            // (`nros_cpp_action_goal_accept_decode`).
            uint8_t wire[17];
            for (int i = 0; i < 16; ++i)
                wire[i] = goal_id[i];
            wire[16] = accepted ? 1 : 0;
            self->accepts_.deliver(owner, wire, sizeof(wire));
        }
        self->upstream_goal_response(accepted, goal_id);
        if (self->user_.goal_response != nullptr)
            self->user_.goal_response(accepted, goal_id, self->user_.context);
    }

    static void on_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx) {
        Client* self = static_cast<Client*>(ctx);
        const size_t n = len < sizeof(self->feedback_buf_) ? len : sizeof(self->feedback_buf_);
        for (size_t i = 0; i < n; ++i)
            self->feedback_buf_[i] = data[i];
        self->feedback_len_ = n;
        self->feedback_truncated_ = n < len;
        self->feedback_ready_ = true;
        self->upstream_feedback(goal_id, data, len);
        if (self->user_.feedback != nullptr)
            self->user_.feedback(goal_id, data, len, self->user_.context);
    }

    static void on_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data,
                          size_t len, void* ctx) {
        Client* self = static_cast<Client*>(ctx);
        ::rclcpp::detail::PendingLink* owner = self->results_.resolve_goal(goal_id);
        if (owner != nullptr) self->results_.deliver(owner, data, len);
        self->upstream_result(goal_id, status, data, len);
        if (self->user_.result != nullptr)
            self->user_.result(goal_id, status, data, len, self->user_.context);
    }

    // Issue 2015 — upstream's per-goal callbacks (`async_send_goal`). Hosted
    // only: `upstream_` is empty on a freestanding build, so these do nothing.
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
    struct UpstreamGoal {
        GoalUUID id;
        SendGoalOptions options;
        typename GoalHandle::SharedPtr handle;
    };
    struct UpstreamState {
        ::std::vector<UpstreamGoal> goals;
    };

    UpstreamState* upstream_goal(const uint8_t goal_id[16], size_t& at) const {
        UpstreamState* st = upstream_.template as<UpstreamState>();
        if (st == nullptr) return nullptr;
        const GoalUUID id(goal_id);
        for (at = 0; at < st->goals.size(); ++at)
            if (st->goals[at].id == id) return st;
        return nullptr;
    }
#endif

    void upstream_goal_response(bool accepted, const uint8_t goal_id[16]) {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
        size_t at = 0;
        UpstreamState* st = upstream_goal(goal_id, at);
        if (st == nullptr) return;
        UpstreamGoal& g = st->goals[at];
        auto cb = g.options.goal_response_callback;
        if (!accepted) {
            st->goals.erase(st->goals.begin() + static_cast<long>(at));
            if (cb) cb(nullptr);
            return;
        }
        g.handle = ::std::make_shared<GoalHandle>(g.id);
        g.handle->feedback_aware_ = static_cast<bool>(g.options.feedback_callback);
        g.handle->result_aware_ = static_cast<bool>(g.options.result_callback);
        typename GoalHandle::SharedPtr handle = g.handle;
        if (!handle->feedback_aware_ && !handle->result_aware_)
            st->goals.erase(st->goals.begin() + static_cast<long>(at));
        if (cb) cb(handle);
        if (handle->result_aware_) (void)get_result_async(goal_id);
#else
        (void)accepted;
        (void)goal_id;
#endif
    }

    void upstream_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len) {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
        size_t at = 0;
        UpstreamState* st = upstream_goal(goal_id, at);
        if (st == nullptr) return;
        auto cb = st->goals[at].options.feedback_callback;
        typename GoalHandle::SharedPtr handle = st->goals[at].handle;
        if (!cb || !handle) return;
        ::std::shared_ptr<FeedbackType> fb = ::std::make_shared<FeedbackType>();
        if (FeedbackType::ffi_deserialize(data, len, fb.get()) != 0) return;
        cb(handle, fb);
#else
        (void)goal_id;
        (void)data;
        (void)len;
#endif
    }

    void upstream_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data,
                         size_t len) {
#ifdef NROS_CPP_HAS_SHARED_PTR // hosted-family: action-interop
        size_t at = 0;
        UpstreamState* st = upstream_goal(goal_id, at);
        if (st == nullptr) return;
        UpstreamGoal g = st->goals[at];
        st->goals.erase(st->goals.begin() + static_cast<long>(at));
        if (g.handle) g.handle->status_ = static_cast<int8_t>(status);
        if (!g.options.result_callback) return;
        WrappedResult wrapped;
        wrapped.goal_id = g.id;
        wrapped.code = (status == 4 || status == 5 || status == 6) ? static_cast<ResultCode>(status)
                                                                   : ResultCode::UNKNOWN;
        wrapped.result = ::std::make_shared<ResultType>();
        if (ResultType::ffi_deserialize(data, len, wrapped.result.get()) != 0) {
            wrapped.result.reset();
        }
        g.options.result_callback(wrapped);
#else
        (void)goal_id;
        (void)status;
        (void)data;
        (void)len;
#endif
    }

    /// The latest feedback this client received (the stream's take hook).
    /// Runs one pass of the client's dispatch first when the slot is empty, so
    /// feedback no `spin_once` has taken yet is found too.
    static nros_cpp_ret_t take_feedback(void* ctx, uint8_t* buf, size_t cap, size_t* out_len) {
        Client* self = static_cast<Client*>(ctx);
        *out_len = 0;
        if (!self->initialized_)
            return static_cast<nros_cpp_ret_t>(::rclcpp::ErrorCode::NotInitialized);
        if (!self->feedback_ready_) (void)nros_cpp_action_client_poll(self->storage_);
        if (!self->feedback_ready_) return 0;
        self->feedback_ready_ = false;
        if (self->feedback_truncated_ || self->feedback_len_ > cap) {
            *out_len = self->feedback_len_;
            return static_cast<nros_cpp_ret_t>(::rclcpp::ErrorCode::Error);
        }
        for (size_t i = 0; i < self->feedback_len_; ++i)
            buf[i] = self->feedback_buf_[i];
        *out_len = self->feedback_len_;
        return 0;
    }

    alignas(8) uint8_t storage_[NROS_CPP_ACTION_CLIENT_STORAGE_SIZE];
    void* executor_; // Stashed executor handle (Phase 82) for blocking helpers
    bool initialized_;
    ::rclcpp::Stream<FeedbackType> feedback_stream_;
    // Phase 87.6 put a `char action_name_[256]` here for an accessor that was
    // never written, and an earlier phase-417 W4.b wave deleted it as dead
    // state. W4.b's second half writes `get_action_name()`, so it is back and
    // it is read — see the note on `Server<A>::get_action_name`.
    char action_name_[::rclcpp_action::ACTION_NAME_MAX];
    /// Issue 2011 -- the futures waiting on this client's acceptances and
    /// results, each keyed by its goal. Intrusive: an entry is a future the
    /// caller holds, so there is no capacity to mis-size and no heap; the Rust
    /// core's per-sub-client table (`MAX_CONCURRENT_GOALS`) refuses a send
    /// past its bound, and the future is then empty.
    ::rclcpp::detail::PendingList accepts_;
    ::rclcpp::detail::PendingList results_;
    /// The user's callbacks (`set_callbacks`), forwarded to after delivery.
    SendGoalOptions user_;
    /// The latest feedback, per client (it was one slot per PROCESS).
    uint8_t feedback_buf_[::rclcpp::rx_buffer_capacity<FeedbackType>::value];
    size_t feedback_len_ = 0;
    bool feedback_ready_ = false;
    bool feedback_truncated_ = false;
    /// Issue 2015 — the goals `async_send_goal` bound upstream callbacks to
    /// (hosted only; empty, and the same two pointers wide, on a freestanding
    /// build).
    detail::HostedBox upstream_;
};

} // namespace rclcpp_action

// Phase 84.G8: out-of-line definition of Node::create_action_client<A>().
#include "rclcpp/node.hpp"

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename A>
Result Node::create_action_client(::rclcpp_action::Client<A>& out, const char* action_name,
                                  const ::rclcpp::QoS& qos) {
    if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
    nros_cpp_qos_t ffi_qos = ::rclcpp::detail::qos_to_ffi(qos);
    const nros_cpp_action_hashes_t hashes = ::rclcpp::detail::action_endpoint_hashes<A>();
    nros_cpp_ret_t ret = nros_cpp_action_client_create(
        &handle_, action_name, A::TYPE_NAME, A::Goal::TYPE_HASH, &hashes, ffi_qos, out.storage_);
    if (ret == 0) {
        out.executor_ = executor_handle_;
        // Issue 2011 -- the client's own callbacks, from the start: they feed
        // its goal-keyed futures and its feedback slot.
        out.initialized_ = true;
        (void)out.install_dispatch();
        // phase-417 W4.b — remember the name for `get_action_name()`, truncating
        // at `rclcpp_action::ACTION_NAME_MAX` exactly as the server and the polling tiers do.
        ::rclcpp::detail::assign_entity_name(out.action_name_, action_name);
        out.initialized_ = true;
    }
    return Result(ret);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

#endif // NROS_RCLCPP_ACTION_CLIENT_HPP
