// FibCallbackClient — the Fibonacci client on the typed rclcpp_action::Client and its
// SendGoalOptions callbacks (RFC-0041). Every line it prints comes from a callback.

#include "action_client_callback_pkg/FibCallbackClient.hpp"

#include <cstdio>
#include <cstring>

namespace action_client_callback_pkg {

void FibCallbackClient::s_goal_response(bool accepted, const uint8_t goal_id[16], void* ctx) {
    auto* self = static_cast<FibCallbackClient*>(ctx);
    if (accepted) {
        std::memcpy(self->goal_id_, goal_id, 16);
        self->phase_ = NeedResult;
        self->waits_ = 0;
        std::printf("Goal accepted by server, waiting for result\n");
    } else {
        std::printf("Goal rejected\n");
        self->phase_ = Done;
    }
}

void FibCallbackClient::s_feedback(const uint8_t goal_id[16], const uint8_t* data, size_t len,
                                   void* ctx) {
    (void)goal_id;
    (void)ctx;
    Action::Feedback fb;
    if (data && len > 0 && Action::Feedback::ffi_deserialize(data, len, &fb) == 0) {
        std::printf("Next number in sequence received: [");
        for (uint32_t i = 0; i < fb.sequence.length(); i++) {
            std::printf(i > 0 ? ", %d" : "%d", fb.sequence[i]);
        }
        std::printf("]\n");
    }
}

void FibCallbackClient::s_result(const uint8_t goal_id[16], int32_t status, const uint8_t* data,
                                 size_t len, void* ctx) {
    (void)goal_id;
    (void)status;
    auto* self = static_cast<FibCallbackClient*>(ctx);
    Action::Result result;
    if (data && len > 0 && Action::Result::ffi_deserialize(data, len, &result) == 0 &&
        result.sequence.length() > 0) {
        std::printf("Result received: [");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            std::printf(i > 0 ? ", %d" : "%d", result.sequence[i]);
        }
        std::printf("]\n");
        self->phase_ = Done;
    }
}

void FibCallbackClient::on_tick() {
    // Drain pending replies into the SendGoalOptions callbacks.
    client_.poll();

    switch (phase_) {
    case Idle: {
        std::printf("%s\n",
                    sends_++ == 0 ? "Sending goal" : "No goal response; sending the goal again");
        Action::Goal goal;
        goal.order = 10;
        if (client_.send_goal_async(goal, goal_id_).ok()) {
            phase_ = GoalSent;
            waits_ = 0;
        }
        break;
    }
    case GoalSent:
        if (++waits_ > 50) {
            phase_ = Idle; // no goal response — resend
        }
        break;
    case NeedResult:
        if (client_.get_result_async(goal_id_).ok()) {
            phase_ = AwaitResult;
            waits_ = 0;
        }
        break;
    case AwaitResult:
        if (++waits_ > 200) {
            phase_ = NeedResult; // no result — re-request
        }
        break;
    case Done:
    default:
        break;
    }
}

::rclcpp::Result FibCallbackClient::configure(::rclcpp::Node& node) {
    ::setvbuf(stdout, nullptr, _IONBF, 0);
    ::rclcpp::Result r = node.create_action_client(client_, "/fibonacci");
    if (!r.ok()) {
        return r;
    }
    ::rclcpp_action::Client<Action>::SendGoalOptions options;
    options.goal_response = &FibCallbackClient::s_goal_response;
    options.feedback = &FibCallbackClient::s_feedback;
    options.result = &FibCallbackClient::s_result;
    options.context = this;
    r = client_.set_callbacks(options);
    if (!r.ok()) {
        return r;
    }
    return node.create_wall_timer<FibCallbackClient, &FibCallbackClient::on_tick>(timer_, 100,
                                                                                  this);
}

} // namespace action_client_callback_pkg
