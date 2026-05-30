// SPDX-License-Identifier: Apache-2.0
//
// Phase 210 mixed-workspace consumer — verbatim ROS 2 C++ source pulling msgs
// from BOTH the workspace AND the ament-installed prefix. Same source compiles
// under colcon (against upstream rclcpp) AND under a nano-ros cmake build
// (rclcpp → rclcpp_compat).
//
// Msg sources exercised:
//   * `local_msgs::msg::Greeting`      — workspace pkg (`src/local_msgs`)
//   * `extra_msgs::msg::Echo`          — workspace pkg, depends on local_msgs
//   * `geometry_msgs::msg::Point`      — AMENT_PREFIX_PATH (`/opt/ros/...`)
//   * `sensor_msgs::msg::Imu`          — AMENT_PREFIX_PATH (+ transitively
//                                        pulls geometry_msgs + std_msgs)
//
// Both worlds resolve through the same `find_package(<pkg>)` call — the smart
// Find-stub walks the layered search path
// (NROS_INTERFACE_SEARCH_PATH > AMENT_PREFIX_PATH > bundled) and routes each
// pkg's codegen identically regardless of which layer it lived in.

#include <chrono>
#include <memory>

#include <rclcpp/rclcpp.hpp>

#include "local_msgs/msg/greeting.hpp"
#include "extra_msgs/msg/echo.hpp"
#include "geometry_msgs/msg/point.hpp"
#include "sensor_msgs/msg/imu.hpp"

using namespace std::chrono_literals;

class MixedConsumer : public rclcpp::Node {
public:
    MixedConsumer() : rclcpp::Node("mixed_consumer"), count_(0) {
        greeting_pub_ = this->create_publisher<local_msgs::msg::Greeting>("greetings", 10);
        echo_pub_     = this->create_publisher<extra_msgs::msg::Echo>("echoes", 10);
        point_pub_    = this->create_publisher<geometry_msgs::msg::Point>("points", 10);
        imu_pub_      = this->create_publisher<sensor_msgs::msg::Imu>("imu", 10);
        timer_ = this->create_wall_timer(500ms, [this]() { this->tick(); });
    }

private:
    void tick() {
        const int32_t seq = static_cast<int32_t>(count_++);

        // Workspace msg (local_msgs).
        local_msgs::msg::Greeting g;
        g.from_who = "mixed-consumer";
        g.sequence = seq;
        greeting_pub_->publish(g);

        // Workspace msg with workspace-cross-dep (extra_msgs → local_msgs).
        extra_msgs::msg::Echo e;
        e.original  = g;
        e.hop_count = 1;
        echo_pub_->publish(e);

        // AMENT msg (geometry_msgs/Point).
        geometry_msgs::msg::Point p;
        p.x = 1.0 * seq;
        p.y = 2.0 * seq;
        p.z = 3.0 * seq;
        point_pub_->publish(p);

        // AMENT msg with AMENT cross-deps (sensor_msgs/Imu → geometry_msgs +
        // std_msgs).
        sensor_msgs::msg::Imu imu;
        imu.linear_acceleration.x = 9.81;
        imu.linear_acceleration.y = 0.0;
        imu.linear_acceleration.z = 0.0;
        imu_pub_->publish(imu);

        RCLCPP_INFO(this->get_logger(),
                    "tick %d — published Greeting/Echo/Point/Imu", seq);
    }

    std::shared_ptr<rclcpp::TimerBase> timer_;
    std::shared_ptr<rclcpp::Publisher<local_msgs::msg::Greeting>> greeting_pub_;
    std::shared_ptr<rclcpp::Publisher<extra_msgs::msg::Echo>>     echo_pub_;
    std::shared_ptr<rclcpp::Publisher<geometry_msgs::msg::Point>> point_pub_;
    std::shared_ptr<rclcpp::Publisher<sensor_msgs::msg::Imu>>     imu_pub_;
    size_t count_;
};

int main(int argc, char* argv[]) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<MixedConsumer>());
    rclcpp::shutdown();
    return 0;
}
