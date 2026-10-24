// nros-cpp: Polling (L1) action client class
// Freestanding C++ — no exceptions, no STL required

/**
 * @file polling_action_client.hpp
 * @ingroup grp_action
 * @brief `rclcpp::PollingActionClient<A>` — caller-polls action client.
 *
 * Phase 122.3.d.b — typed C++ wrapper over the L1 polling FFI added
 * in 122.3.d.a. Mirrors `ActionClient<A>` but drops the executor
 * arena / callback model: the caller drives `send_goal_raw` +
 * `take_*` from a spin loop.
 */

#ifndef NROS_RCLCPP_POLLING_ACTION_CLIENT_HPP
#define NROS_RCLCPP_POLLING_ACTION_CLIENT_HPP

#include <cstddef>
#include <cstdint>
#include <string.h>

// phase-417 W4.b — `rclcpp_action::GoalUUID`, the goal-id VALUE type, is declared
// beside the shared action enums in action_server.hpp (which is also where
// polling_action_server.hpp reaches for them).
#include "rclcpp_action/server.hpp"
#include "rclcpp/detail/config.hpp"
#include "rclcpp/detail/entity_name.hpp" // phase-444 — the one entity-name copy
#include "rclcpp/node.hpp"
#include "rclcpp/detail/nros_cpp_config_generated.h"
#include "rclcpp/result.hpp"
#include "rclcpp/size_bound.hpp" // rclcpp::rx_buffer_capacity<M> — the receive-buffer size

#include "rclcpp/detail/nros_cpp_ffi.h"

// phase-427 W7 — `Node` is DEFINED in `rclcpp::` (RFC-0089: that namespace is
// the home). The friend declaration below is qualified, and a qualified friend
// names an existing entity rather than introducing one, so the name has to be
// declared first — and in `rclcpp::`, because an elaborated `class Node;` in
// `rclcpp::` would declare a second, distinct class.
namespace rclcpp {
class Node;
}

namespace rclcpp {

/// Typed L1 polling-mode action client.
///
/// Usage:
/// ```cpp
/// using Fib = example_interfaces::action::Fibonacci;
/// rclcpp::PollingActionClient<Fib> cli;
/// RCLCPP_TRY(node.create_polling_action_client(cli, "/fibonacci"));
/// uint8_t goal_id[16];
/// typename Fib::Goal g;  g.order = 10;
/// cli.send_goal(g, goal_id);
/// while (running) {
///     typename Fib::Feedback fb; uint8_t fid[16];
///     if (cli.try_recv_feedback(fid, fb).ok()) { /* … */ }
///     typename Fib::Result r;
///     if (cli.try_recv_result(r).ok()) { break; }
/// }
/// ```
template <typename A> class PollingActionClient {
  public:
    /// phase-417 W4.b — one spelling for all four action classes; see
    /// `rclcpp_action::ACTION_NAME_MAX` in `action_server.hpp`. Kept as a member
    /// name so existing `PollingActionServer<A>::ACTION_NAME_MAX` /
    /// `PollingActionClient<A>::ACTION_NAME_MAX` call sites still resolve.
    static constexpr size_t ACTION_NAME_MAX = ::rclcpp_action::ACTION_NAME_MAX;

    using GoalType = typename A::Goal;
    using ResultType = typename A::Result;
    using FeedbackType = typename A::Feedback;

    PollingActionClient() : storage_{}, action_name_{}, initialized_(false) {}

    ~PollingActionClient() {
        if (initialized_) {
            nros_cpp_action_client_destroy_polling(storage_);
            initialized_ = false;
        }
    }

    PollingActionClient(const PollingActionClient&) = delete;
    PollingActionClient& operator=(const PollingActionClient&) = delete;

    bool is_valid() const { return initialized_; }
    const char* get_action_name() const { return initialized_ ? action_name_ : ""; }

    /// Send a typed goal. Writes generated 16-byte UUID into
    /// `goal_id_out`.
    Result send_goal(const GoalType& goal, uint8_t goal_id_out[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        uint8_t buf[::rclcpp::detail::buffer_bounds<GoalType>::tx];
        size_t len = 0;
        if (GoalType::ffi_serialize(&goal, buf, sizeof(buf), &len) != 0)
            return Result(ErrorCode::Error);
        return Result(nros_cpp_action_client_send_goal_raw(
            storage_, buf, len, reinterpret_cast<uint8_t(*)[16]>(goal_id_out)));
    }

    /// @ref send_goal writing the generated id into a `GoalUUID` value
    /// (phase-417 W4.b).
    Result send_goal(const GoalType& goal, ::rclcpp_action::GoalUUID& goal_id_out) {
        return send_goal(goal, goal_id_out.data());
    }

    /// Try to receive the send_goal RPC reply (accept / reject).
    /// Caller deserializes the wire CDR as needed; this template
    /// just returns the raw bytes count in `out_len`.
    Result try_recv_goal_response(uint8_t* buf, size_t cap, size_t& out_len) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        int32_t rc = nros_cpp_action_client_try_recv_goal_response_raw(storage_, buf, cap);
        if (rc < 0) return Result(static_cast<nros_cpp_ret_t>(rc));
        if (rc == 0) {
            out_len = 0;
            return Result(ErrorCode::TryAgain);
        }
        out_len = static_cast<size_t>(rc);
        return Result::success();
    }

    /// Issue 2011 -- @ref try_recv_goal_response that also writes the goal the
    /// reply is ABOUT into `goal_id_out` (the goal of the request whose
    /// sequence id it names, issue 2009). With several goals in flight the
    /// bytes alone cannot say which one a reply answers.
    Result try_recv_goal_response(uint8_t* buf, size_t cap, size_t& out_len,
                                  uint8_t goal_id_out[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        int32_t rc = nros_cpp_action_client_try_recv_goal_response_raw_for(
            storage_, buf, cap, reinterpret_cast<uint8_t(*)[16]>(goal_id_out));
        return raw_len_result(rc, out_len);
    }

    /// @ref try_recv_goal_response writing the goal into a `GoalUUID`.
    Result try_recv_goal_response(uint8_t* buf, size_t cap, size_t& out_len,
                                  ::rclcpp_action::GoalUUID& goal_id_out) {
        return try_recv_goal_response(buf, cap, out_len, goal_id_out.data());
    }

    /// Issue a get_result request for the given goal.
    Result send_get_result_request(const uint8_t goal_id[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_send_get_result_request_raw(
            storage_, reinterpret_cast<const uint8_t(*)[16]>(goal_id)));
    }

    /// @ref send_get_result_request taking a `GoalUUID` value (phase-417 W4.b).
    Result send_get_result_request(const ::rclcpp_action::GoalUUID& goal_id) {
        return send_get_result_request(goal_id.data());
    }

    /// Try to receive the get_result reply as a typed result.
    /// The reply has wire layout `[CDR header 4][status i8][pad → byte 8]
    /// [result fields]` (`NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET`) —
    /// deserializes the fields, behind the reply's own header, into `out`.
    Result try_recv_result(ResultType& out) {
        return try_recv_result_sized<::rclcpp::rx_buffer_capacity<ResultType>::value>(out);
    }

    /// @ref try_recv_result with the RESULT PAYLOAD capacity chosen by the
    /// caller. `Cap` is the payload, not the buffer: the wire reply carries an
    /// 8-byte prefix (CDR header + status + alignment pad,
    /// `NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET`), which this adds (issues 0964,
    /// 2014).
    /// See @ref Subscription::try_recv_sized.
    template <size_t Cap> Result try_recv_result_sized(ResultType& out) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        uint8_t buf[Cap + kResultOffset];
        int32_t rc = nros_cpp_action_client_try_recv_result_raw(storage_, buf, sizeof(buf));
        return decode_result(rc, buf, out);
    }

    /// Issue 2011 -- @ref try_recv_result that also writes the goal the result
    /// is FOR into `goal_id_out` (see the goal-response overload).
    Result try_recv_result(ResultType& out, uint8_t goal_id_out[16]) {
        return try_recv_result_sized<::rclcpp::rx_buffer_capacity<ResultType>::value>(out,
                                                                                      goal_id_out);
    }

    /// @ref try_recv_result writing the goal into a `GoalUUID`.
    Result try_recv_result(ResultType& out, ::rclcpp_action::GoalUUID& goal_id_out) {
        return try_recv_result(out, goal_id_out.data());
    }

    /// @ref try_recv_result_sized with the goal written to `goal_id_out`.
    template <size_t Cap> Result try_recv_result_sized(ResultType& out, uint8_t goal_id_out[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        uint8_t buf[Cap + kResultOffset];
        int32_t rc = nros_cpp_action_client_try_recv_result_raw_for(
            storage_, buf, sizeof(buf), reinterpret_cast<uint8_t(*)[16]>(goal_id_out));
        return decode_result(rc, buf, out);
    }

    /// Issue a cancel request for the given goal.
    Result send_cancel_request(const uint8_t goal_id[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_send_cancel_request_raw(
            storage_, reinterpret_cast<const uint8_t(*)[16]>(goal_id)));
    }

    /// @ref send_cancel_request taking a `GoalUUID` value (phase-417 W4.b).
    Result send_cancel_request(const ::rclcpp_action::GoalUUID& goal_id) {
        return send_cancel_request(goal_id.data());
    }

    /// Try to receive the cancel RPC reply (raw CDR bytes).
    Result try_recv_cancel_response(uint8_t* buf, size_t cap, size_t& out_len) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        int32_t rc = nros_cpp_action_client_try_recv_cancel_response_raw(storage_, buf, cap);
        if (rc < 0) return Result(static_cast<nros_cpp_ret_t>(rc));
        if (rc == 0) {
            out_len = 0;
            return Result(ErrorCode::TryAgain);
        }
        out_len = static_cast<size_t>(rc);
        return Result::success();
    }

    /// Issue 2011 -- @ref try_recv_cancel_response that also writes the goal
    /// the reply is ABOUT into `goal_id_out`.
    Result try_recv_cancel_response(uint8_t* buf, size_t cap, size_t& out_len,
                                    uint8_t goal_id_out[16]) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        int32_t rc = nros_cpp_action_client_try_recv_cancel_response_raw_for(
            storage_, buf, cap, reinterpret_cast<uint8_t(*)[16]>(goal_id_out));
        return raw_len_result(rc, out_len);
    }

    /// Try to receive a feedback message. Writes the source `goal_id`
    /// (16 bytes) and deserializes the payload into `out_fb`.
    Result try_recv_feedback(uint8_t goal_id_out[16], FeedbackType& out_fb) {
        return try_recv_feedback_sized<::rclcpp::rx_buffer_capacity<FeedbackType>::value>(
            goal_id_out, out_fb);
    }

    /// @ref try_recv_feedback writing the source id into a `GoalUUID` value
    /// (phase-417 W4.b).
    Result try_recv_feedback(::rclcpp_action::GoalUUID& goal_id_out, FeedbackType& out_fb) {
        return try_recv_feedback_sized<::rclcpp::rx_buffer_capacity<FeedbackType>::value>(
            goal_id_out.data(), out_fb);
    }

    /// @ref try_recv_feedback_sized taking a `GoalUUID` value.
    template <size_t Cap>
    Result try_recv_feedback_sized(::rclcpp_action::GoalUUID& goal_id_out, FeedbackType& out_fb) {
        return try_recv_feedback_sized<Cap>(goal_id_out.data(), out_fb);
    }

    /// @ref try_recv_feedback with the receive buffer sized by the CALLER.
    /// See @ref Subscription::try_recv_sized (issue 0964).
    template <size_t Cap>
    Result try_recv_feedback_sized(uint8_t goal_id_out[16], FeedbackType& out_fb) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        // Issue 2014 — the raw frame is `[CDR header 4][GoalId 16][fields]`,
        // so the buffer holds `Cap` of FIELDS plus that prefix, and the
        // fields are decoded behind the frame's own header (they used to be
        // decoded from byte 0, reading the goal id as the payload).
        uint8_t buf[Cap + kFeedbackOffset];
        int32_t rc = nros_cpp_action_client_try_recv_feedback_raw(
            storage_, buf, sizeof(buf), reinterpret_cast<uint8_t(*)[16]>(goal_id_out));
        return decode_spliced<FeedbackType>(rc, buf, kFeedbackOffset, out_fb);
    }

    /// Phase 122.3.c.6.e — caller-owned wake-state slot. One per
    /// (channel) pair; declare next to the client, pass into
    /// `set_*_wake_callback`. Must outlive the client.
    struct WakeState {
        alignas(8) uint64_t _opaque[2] = {0, 0};
    };

    Result set_goal_response_wake_callback(WakeState& state, void (*cb)(void*), void* ctx) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_set_goal_response_wake_callback(
            storage_, reinterpret_cast<nros_cpp_wake_state_t*>(&state),
            reinterpret_cast<void (*)(void*)>(cb), ctx));
    }

    Result set_cancel_response_wake_callback(WakeState& state, void (*cb)(void*), void* ctx) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_set_cancel_response_wake_callback(
            storage_, reinterpret_cast<nros_cpp_wake_state_t*>(&state),
            reinterpret_cast<void (*)(void*)>(cb), ctx));
    }

    Result set_result_wake_callback(WakeState& state, void (*cb)(void*), void* ctx) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_set_result_wake_callback(
            storage_, reinterpret_cast<nros_cpp_wake_state_t*>(&state),
            reinterpret_cast<void (*)(void*)>(cb), ctx));
    }

    Result set_feedback_wake_callback(WakeState& state, void (*cb)(void*), void* ctx) {
        if (!initialized_) return Result(ErrorCode::NotInitialized);
        return Result(nros_cpp_action_client_set_feedback_wake_callback(
            storage_, reinterpret_cast<nros_cpp_wake_state_t*>(&state),
            reinterpret_cast<void (*)(void*)>(cb), ctx));
    }

  private:
    friend class ::rclcpp::Node;

    /// A raw `_raw[_for]` return: `<0` error, `0` nothing yet, `>0` bytes.
    static Result raw_len_result(int32_t rc, size_t& out_len) {
        if (rc < 0) return Result(static_cast<nros_cpp_ret_t>(rc));
        if (rc == 0) {
            out_len = 0;
            return Result(ErrorCode::TryAgain);
        }
        out_len = static_cast<size_t>(rc);
        return Result::success();
    }

    /// Issue 2014 — the action frame offsets, from the ONE spelling
    /// (`nros_node::action_frame`, mirrored into the C ABI and asserted equal
    /// there). This header used to skip 5 bytes of a get_result reply whose
    /// payload starts at 8.
    static constexpr size_t kHeaderLen = NROS_CPP_ACTION_CDR_HEADER_LEN;
    static constexpr size_t kResultOffset = NROS_CPP_ACTION_RESULT_PAYLOAD_OFFSET;
    static constexpr size_t kFeedbackOffset = NROS_CPP_ACTION_FEEDBACK_PAYLOAD_OFFSET;
    static_assert(kResultOffset >= kHeaderLen && kFeedbackOffset >= kHeaderLen,
                  "an action payload offset lies after the CDR header");

    /// Decode a raw action frame of `rc` bytes in `buf` whose fields start at
    /// `offset`: the frame's own CDR header is copied IN PLACE to sit right
    /// before the fields, and `[header][fields]` is handed to the
    /// deserializer — the same splice every executor dispatch does
    /// (`action_frame::splice`), with no second buffer.
    template <typename M>
    static Result decode_spliced(int32_t rc, uint8_t* buf, size_t offset, M& out) {
        if (rc < 0) return Result(static_cast<nros_cpp_ret_t>(rc));
        if (rc == 0) return Result(ErrorCode::TryAgain);
        if (static_cast<size_t>(rc) < offset) return Result(ErrorCode::Error);
        uint8_t* framed = buf + (offset - kHeaderLen);
        for (size_t i = 0; i < kHeaderLen; ++i)
            framed[i] = buf[i];
        size_t framed_len = static_cast<size_t>(rc) - (offset - kHeaderLen);
        if (M::ffi_deserialize(framed, framed_len, &out) != 0) return Result(ErrorCode::Error);
        return Result::success();
    }

    /// Decode a get_result reply of `rc` bytes in `buf` into `out`.
    static Result decode_result(int32_t rc, uint8_t* buf, ResultType& out) {
        return decode_spliced<ResultType>(rc, buf, kResultOffset, out);
    }

    static constexpr size_t kStorageU64s = NROS_CPP_RAW_ACTION_CLIENT_OPAQUE_U64S;
    alignas(8) uint64_t storage_[kStorageU64s];
    char action_name_[ACTION_NAME_MAX];
    bool initialized_;
};

} // namespace rclcpp

#include "rclcpp/node.hpp"

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename A>
Result Node::create_polling_action_client(::rclcpp::PollingActionClient<A>& out,
                                          const char* action_name) {
    if (!initialized_) return Result(::rclcpp::ErrorCode::NotInitialized);
    const nros_cpp_action_hashes_t hashes = ::rclcpp::detail::action_endpoint_hashes<A>();
    nros_cpp_ret_t ret =
        nros_cpp_action_client_init_polling(&handle_, action_name, A::TYPE_NAME, A::Goal::TYPE_HASH,
                                            &hashes, reinterpret_cast<void*>(out.storage_));
    if (ret != 0) return Result(ret);
    ::rclcpp::detail::assign_entity_name(out.action_name_, action_name);
    out.initialized_ = true;
    return Result::success();
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

#endif // NROS_RCLCPP_POLLING_ACTION_CLIENT_HPP
