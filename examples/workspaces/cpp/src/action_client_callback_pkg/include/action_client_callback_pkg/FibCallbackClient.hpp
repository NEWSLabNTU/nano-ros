#pragma once

#include <cstddef>
#include <cstdint>

#include <rclcpp/rclcpp.hpp>

#include "example_interfaces.hpp"

namespace action_client_callback_pkg {

/// FibCallbackClient — the tutorial Fibonacci client (one order-10 goal; one
/// `Next number in sequence received: [...]` line per feedback sample, then
/// `Result received: [...]`), **callback** variant (RFC-0041): it is written against the TYPED
/// `rclcpp_action::Client<A>`, and the goal response, feedback and result arrive through the
/// `SendGoalOptions` callbacks `set_callbacks` registers. `action_client_pkg::FibClient` runs the
/// same program on the raw FFI. phase-535 W6 (RFC-0122 D2) made the old
/// `examples/linux/cpp/action-client-callback` leaf this node package.
///
/// The action client's replies arrive as GET-query replies that `poll()` drains into the
/// callbacks, and `spin_once` does not call it (issue 0047), so a 100 ms timer polls and runs a
/// small state machine. A component callback must never block the executor, so every wait is a
/// count of ticks, and a goal or result request with no reply is sent again.
class FibCallbackClient {
    using Action = example_interfaces::action::Fibonacci;

    enum Phase {
        Idle = 0,        // no goal in flight — send one
        GoalSent = 1,    // goal sent — await the goal-response callback
        NeedResult = 2,  // accepted — request the result from the tick
        AwaitResult = 3, // result requested — await the result callback
        Done = 4         // result received + printed
    };

    ::rclcpp_action::Client<Action> client_;
    ::rclcpp::Timer timer_;
    uint8_t goal_id_[16] = {};
    int phase_ = Idle;
    int waits_ = 0; // ticks waited in the current phase (resend guard)
    int sends_ = 0; // goals sent so far (the first prints `Sending goal`)

    static void s_goal_response(bool accepted, const uint8_t goal_id[16], void* ctx);
    static void s_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len, void* ctx);
    static void s_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data, size_t len,
                         void* ctx);

    void on_tick();

  public:
    ::rclcpp::Result configure(::rclcpp::Node& node);
};

} // namespace action_client_callback_pkg
