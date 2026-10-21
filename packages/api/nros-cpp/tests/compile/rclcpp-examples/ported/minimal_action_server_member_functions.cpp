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

#include <functional>
#include <memory>
#include <thread>

#include "example_interfaces/action/fibonacci.hpp"
#include "rclcpp/rclcpp.hpp"
// TODO(jacobperron): Remove this once it is included as part of 'rclcpp.hpp'
#include "rclcpp_action/rclcpp_action.hpp"

class MinimalActionServer : public rclcpp::Node
{
public:
  using Fibonacci = example_interfaces::action::Fibonacci;

  explicit MinimalActionServer(const rclcpp::NodeOptions & options = rclcpp::NodeOptions())
  : Node("minimal_action_server", options)
  {
    (void)this->create_action_server(this->action_server_, "fibonacci");
    (void)this->action_server_.set_goal_callback_with_ctx(&MinimalActionServer::handle_goal, this);
    (void)this->action_server_.set_cancel_callback_with_ctx(
      &MinimalActionServer::handle_cancel, this);
    (void)this->action_server_.set_accepted_callback_with_ctx(
      &MinimalActionServer::handle_accepted, this);
  }

private:
  rclcpp_action::Server<Fibonacci> action_server_;
  int32_t order_ = 0;
  bool canceling_ = false;

  static rclcpp_action::GoalResponse handle_goal(
    const uint8_t uuid[16],
    const Fibonacci::Goal & goal, void * ctx)
  {
    auto * self = static_cast<MinimalActionServer *>(ctx);
    RCLCPP_INFO(self->get_logger(), "Received goal request with order %d", goal.order);
    (void)uuid;
    // Let's reject sequences that are over 9000
    if (goal.order > 9000) {
      return rclcpp_action::GoalResponse::REJECT;
    }
    self->order_ = goal.order;
    self->canceling_ = false;
    return rclcpp_action::GoalResponse::ACCEPT_AND_EXECUTE;
  }

  static rclcpp_action::CancelResponse handle_cancel(const uint8_t uuid[16], void * ctx)
  {
    auto * self = static_cast<MinimalActionServer *>(ctx);
    RCLCPP_INFO(self->get_logger(), "Received request to cancel goal");
    (void)uuid;
    self->canceling_ = true;
    return rclcpp_action::CancelResponse::ACCEPT;
  }

  void execute(const uint8_t uuid[16])
  {
    RCLCPP_INFO(this->get_logger(), "Executing goal");
    Fibonacci::Feedback feedback;
    auto & sequence = feedback.sequence;
    sequence.push_back(0);
    sequence.push_back(1);
    Fibonacci::Result result;

    for (int i = 1; (i < order_) && rclcpp::ok(); ++i) {
      // Check if there is a cancel request
      if (canceling_) {
        result.sequence = sequence;
        (void)action_server_.canceled(uuid, result);
        RCLCPP_INFO(this->get_logger(), "Goal Canceled");
        return;
      }
      // Update sequence
      sequence.push_back(sequence[i] + sequence[i - 1]);
      // Publish feedback
      (void)action_server_.publish_feedback(uuid, feedback);
      RCLCPP_INFO(this->get_logger(), "Publish Feedback");
    }

    // Check if goal is done
    if (rclcpp::ok()) {
      result.sequence = sequence;
      (void)action_server_.succeed(uuid, result);
      RCLCPP_INFO(this->get_logger(), "Goal Succeeded");
    }
  }

  static void handle_accepted(const uint8_t uuid[16], void * ctx)
  {
    // No thread: the goal runs to completion inside the accepted callback.
    static_cast<MinimalActionServer *>(ctx)->execute(uuid);
  }
};  // class MinimalActionServer

int main(int argc, char ** argv)
{
  rclcpp::init(argc, argv);

  auto action_server = std::make_shared<MinimalActionServer>();

  rclcpp::spin(action_server);

  rclcpp::shutdown();
  return 0;
}
