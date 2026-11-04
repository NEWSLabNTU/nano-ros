// FibServer — typed Fibonacci action server bound by identity (no hand-rolled CDR). The ROS 2
// tutorial server: prints `Received goal request`, `Executing goal`, one `Publish feedback` per
// element and `Goal succeeded`. phase-535 made this the ONE C++ server (RFC-0122 option B).

#include "action_server_pkg/FibServer.hpp"

#include <cstdio>
#include <cstring>

namespace action_server_pkg {

/// Goal callback — receives the goal UUID + the goal's CDR bytes. Parses the order, ACCEPTS, and
/// stashes the goal for the timer to complete. Returns the typed `GoalResponse`; the binder
/// converts it to the FFI discriminant (`rclcpp::to_ffi`).
::rclcpp_action::GoalResponse FibServer::on_goal(const uint8_t goal_id[16], const uint8_t* data,
                                                 size_t len) {
    Action::Goal goal;
    if (Action::Goal::ffi_deserialize(data, len, &goal) != 0) {
        /* issue 0737 — say why. A silent reject is indistinguishable from
         * a request that never arrived. */
        std::fprintf(stderr,
                     "[action_server_pkg] goal REJECTED — deserialize failed, %zu byte(s)\n", len);
        return ::rclcpp_action::GoalResponse::REJECT;
    }
    if (goal.order < 0 || goal.order >= 64) {
        std::printf("Goal rejected: order out of range\n");
        return ::rclcpp_action::GoalResponse::REJECT;
    }

    std::memcpy(goal_id_, goal_id, 16);
    order_ = goal.order;
    has_pending_ = true;
    std::printf("Received goal request with order %d\n", goal.order);
    return ::rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE;
}

::rclcpp_action::CancelResponse FibServer::on_cancel(const uint8_t goal_id[16]) {
    (void)goal_id;
    return ::rclcpp_action::CancelResponse::ACCEPT;
}

/// Timer tick — the only place the executor is free for action ops. Computes the Fibonacci
/// sequence (order + 1 elements: 0,1,1,2,3,… so order 10 → last element 55) for the accepted
/// goal, publishing the sequence so far as feedback after each element, and completes the goal
/// with the result CDR.
void FibServer::on_tick() {
    if (!has_pending_) {
        return;
    }

    std::printf("Executing goal\n");
    Action::Result result;
    Action::Feedback feedback;
    int32_t a = 0;
    int32_t b = 1;
    for (int32_t i = 0; i <= order_; i++) {
        result.sequence.push_back(a);
        feedback.sequence.push_back(a);
        uint8_t fb_buf[Action::Feedback::SERIALIZED_SIZE_MAX];
        size_t fb_len = 0;
        if (Action::Feedback::ffi_serialize(&feedback, fb_buf, sizeof(fb_buf), &fb_len) == 0 &&
            nros_cpp_action_server_publish_feedback(storage_.bytes, executor_, &goal_id_, fb_buf,
                                                    fb_len) == 0) {
            std::printf("Publish feedback\n");
        } else {
            std::fprintf(stderr, "Failed to publish feedback\n");
        }
        int32_t next = a + b;
        a = b;
        b = next;
    }

    uint8_t buf[Action::Result::SERIALIZED_SIZE_MAX];
    size_t written = 0;
    if (Action::Result::ffi_serialize(&result, buf, sizeof(buf), &written) != 0) {
        return;
    }
    if (nros_cpp_action_server_complete_goal(
            storage_.bytes, executor_, &goal_id_,
            static_cast<int32_t>(rclcpp_action::GoalStatus::Succeeded), buf, written) == 0) {
        has_pending_ = false;
        goal_count_++;
        std::printf("Goal succeeded\n");
    }
}

::rclcpp::Result FibServer::configure(::rclcpp::Node& node) {
    // `::setvbuf` (C global), not `std::setvbuf` — Zephyr picolibc lacks the std:: name.
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    executor_ = node.executor_handle();
    has_pending_ = false;

    ::rclcpp::Result r =
        ::rclcpp::bind_action_server_raw<FibServer, &FibServer::on_goal, &FibServer::on_cancel>(
            node, storage_.bytes, "/fibonacci", Action::TYPE_NAME, this);
    if (!r.ok()) {
        return r;
    }
    r = node.create_wall_timer<FibServer, &FibServer::on_tick>(timer_, 500, this);
    if (r.ok()) {
        std::printf("Waiting for action goals\n");
    }
    return r;
}

} // namespace action_server_pkg
