// Copyright 2019 Open Source Robotics Foundation, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#include <chrono>
#include <cinttypes>
#include <functional>
#include <future>
#include <memory>
#include <string>

#include "example_interfaces/action/fibonacci.hpp"
#include "rclcpp/rclcpp.hpp"
// TODO(jacobperron): Remove this once it is included as part of 'rclcpp.hpp'
#include "rclcpp_action/rclcpp_action.hpp"

class MinimalActionClient : public rclcpp::Node
{
public:
  using Fibonacci = example_interfaces::action::Fibonacci;

  explicit MinimalActionClient(const rclcpp::NodeOptions & node_options = rclcpp::NodeOptions())
  : Node("minimal_action_client", node_options), goal_done_(false)
  {
    client_ok_ = this->create_action_client(this->client_, "fibonacci").ok();

    this->timer_ = this->create_wall_timer(
      std::chrono::milliseconds(500),
      std::bind(&MinimalActionClient::send_goal, this));
  }

  bool is_goal_done() const
  {
    return this->goal_done_;
  }

  void send_goal()
  {
    this->timer_->cancel();

    this->goal_done_ = false;

    if (!this->client_ok_) {
      RCLCPP_ERROR(this->get_logger(), "Action client not initialized");
    }

    if (!this->client_.wait_for_action_server(std::chrono::seconds(10))) {
      RCLCPP_ERROR(this->get_logger(), "Action server not available after waiting");
      this->goal_done_ = true;
      return;
    }

    auto goal_msg = Fibonacci::Goal();
    goal_msg.order = 10;

    RCLCPP_INFO(this->get_logger(), "Sending goal");

    auto send_goal_options = rclcpp_action::Client<Fibonacci>::SendGoalOptions();
    send_goal_options.goal_response = &MinimalActionClient::goal_response_callback;
    send_goal_options.feedback = &MinimalActionClient::feedback_callback;
    send_goal_options.result = &MinimalActionClient::result_callback;
    send_goal_options.context = this;
    (void)this->client_.set_callbacks(send_goal_options);
    uint8_t goal_id[16];
    (void)this->client_.send_goal_async(goal_msg, goal_id);
  }

private:
  rclcpp_action::Client<Fibonacci> client_;
  bool client_ok_ = false;
  rclcpp::TimerBase::SharedPtr timer_;
  bool goal_done_;

  static void goal_response_callback(bool accepted, const uint8_t goal_id[16], void * ctx)
  {
    auto * self = static_cast<MinimalActionClient *>(ctx);
    if (!accepted) {
      RCLCPP_ERROR(self->get_logger(), "Goal was rejected by server");
    } else {
      RCLCPP_INFO(self->get_logger(), "Goal accepted by server, waiting for result");
      (void)self->client_.get_result_async(goal_id);
    }
  }

  static void feedback_callback(
    const uint8_t goal_id[16], const uint8_t * data, size_t len, void * ctx)
  {
    auto * self = static_cast<MinimalActionClient *>(ctx);
    (void)goal_id;
    Fibonacci::Feedback feedback;
    if (Fibonacci::Feedback::ffi_deserialize(data, len, &feedback) != 0) {
      return;
    }
    RCLCPP_INFO(
      self->get_logger(),
      "Next number in sequence received: %" PRId32,
      feedback.sequence[feedback.sequence.length() - 1]);
  }

  static void result_callback(
    const uint8_t goal_id[16], int32_t status, const uint8_t * data, size_t len, void * ctx)
  {
    auto * self = static_cast<MinimalActionClient *>(ctx);
    (void)goal_id;
    self->goal_done_ = true;
    switch (static_cast<rclcpp_action::GoalStatus>(status)) {
      case rclcpp_action::GoalStatus::Succeeded:
        break;
      case rclcpp_action::GoalStatus::Aborted:
        RCLCPP_ERROR(self->get_logger(), "Goal was aborted");
        return;
      case rclcpp_action::GoalStatus::Canceled:
        RCLCPP_ERROR(self->get_logger(), "Goal was canceled");
        return;
      default:
        RCLCPP_ERROR(self->get_logger(), "Unknown result code");
        return;
    }

    Fibonacci::Result result;
    if (Fibonacci::Result::ffi_deserialize(data, len, &result) != 0) {
      return;
    }
    RCLCPP_INFO(self->get_logger(), "Result received");
    for (auto number : result.sequence) {
      RCLCPP_INFO(self->get_logger(), "%" PRId32, number);
    }
  }
};  // class MinimalActionClient

int main(int argc, char ** argv)
{
  rclcpp::init(argc, argv);
  auto action_client = std::make_shared<MinimalActionClient>();

  while (!action_client->is_goal_done()) {
    rclcpp::spin_some(action_client);
  }

  rclcpp::shutdown();
  return 0;
}
