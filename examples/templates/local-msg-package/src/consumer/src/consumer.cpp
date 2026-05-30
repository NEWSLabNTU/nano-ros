// SPDX-License-Identifier: Apache-2.0
//
// Phase 210.A.4 consumer — verbatim ROS 2 C++ source using the locally-defined
// `local_msgs/msg/Greeting` type. Same source builds under colcon (against
// upstream rclcpp) AND under a nano-ros cmake build (rclcpp → rclcpp_compat).

#include <chrono>
#include <memory>

#include <rclcpp/rclcpp.hpp>

#include "local_msgs/msg/greeting.hpp"

using namespace std::chrono_literals;

class GreetingPublisher : public rclcpp::Node {
public:
    GreetingPublisher() : rclcpp::Node("greeting_publisher"), count_(0) {
        publisher_ = this->create_publisher<local_msgs::msg::Greeting>("greetings", 10);
        timer_ = this->create_wall_timer(500ms, [this]() { this->tick(); });
    }

private:
    void tick() {
        local_msgs::msg::Greeting m;
        m.from_who = "phase-210-fixture";
        m.sequence = static_cast<int32_t>(count_++);
        RCLCPP_INFO(this->get_logger(), "Greeting %d from %s", m.sequence, m.from_who.c_str());
        publisher_->publish(m);
    }

    std::shared_ptr<rclcpp::TimerBase> timer_;
    std::shared_ptr<rclcpp::Publisher<local_msgs::msg::Greeting>> publisher_;
    size_t count_;
};

int main(int argc, char* argv[]) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<GreetingPublisher>());
    rclcpp::shutdown();
    return 0;
}
