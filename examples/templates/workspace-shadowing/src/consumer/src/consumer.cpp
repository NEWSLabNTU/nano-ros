// SPDX-License-Identifier: Apache-2.0
//
// Phase 210.F.4 shadowing-fixture consumer.
//
// Resolves `find_package(std_msgs)` through the workspace `src/std_msgs/`
// shadow (via NROS_INTERFACE_SEARCH_PATH). This file's mere compile is
// the strongest shadowing proof: upstream ROS 2 `std_msgs` does NOT
// ship `Marker.msg`, so if the layered resolver fell through to AMENT
// Layer 2, the `#include "std_msgs/msg/marker.hpp"` below would fail.
//
// The `marker.shadowed_marker = ...` line forces the workspace-only
// field name into the consumer's symbol closure — `nm` on the linked
// binary shows `std_msgs::msg::Marker_*` symbols referencing the
// `shadowed_marker` accessor (vs. the upstream std_msgs's Header /
// Bool / Empty / Float* / Int* / String / etc.).

#include <chrono>
#include <memory>

#include <rclcpp/rclcpp.hpp>

#include "std_msgs/msg/marker.hpp"

using namespace std::chrono_literals;

class ShadowConsumer : public rclcpp::Node {
public:
    ShadowConsumer() : rclcpp::Node("shadow_consumer") {
        publisher_ =
            this->create_publisher<std_msgs::msg::Marker>("markers", 10);
        timer_ = this->create_wall_timer(500ms, [this]() {
            std_msgs::msg::Marker marker;
            // Field name unique to the workspace shadow — upstream
            // std_msgs ships no Marker.msg at all.
            marker.shadowed_marker = "from-workspace-std_msgs";
            publisher_->publish(marker);
        });
    }

private:
    std::shared_ptr<rclcpp::TimerBase> timer_;
    std::shared_ptr<rclcpp::Publisher<std_msgs::msg::Marker>> publisher_;
};

int main(int argc, char* argv[]) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<ShadowConsumer>());
    rclcpp::shutdown();
    return 0;
}
