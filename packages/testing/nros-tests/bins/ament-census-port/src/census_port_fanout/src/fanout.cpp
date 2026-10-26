// Issue 2043 — a node that outgrows the embedded defaults: six subscriptions,
// six publishers and two timers are eight executor callbacks against a default
// of four. Sized from its census, it starts; left at the defaults, it fails
// at its fifth callback.
#include <array>
#include <chrono>
#include <memory>
#include <string>

#include "rclcpp/rclcpp.hpp"
#include "std_msgs/msg/int32.hpp"

using namespace std::chrono_literals;

namespace census_port_fanout {

class Fanout : public rclcpp::Node {
  public:
    Fanout() : rclcpp::Node("census_port_fanout") {
        for (size_t i = 0; i < kWidth; ++i) {
            const std::string n = std::to_string(i);
            pubs_[i] = create_publisher<std_msgs::msg::Int32>("out_" + n, 10);
            subs_[i] = create_subscription<std_msgs::msg::Int32>(
                "in_" + n, 10, [this, i](const std_msgs::msg::Int32& m) { pubs_[i]->publish(m); });
        }
        fast_ = create_wall_timer(100ms, [this]() { ++ticks_; });
        slow_ = create_wall_timer(1s, [this]() {
            RCLCPP_INFO(get_logger(), "census_port_fanout alive: %d ticks", ticks_);
        });
        RCLCPP_INFO(get_logger(), "census_port_fanout up: %zu subscriptions, %zu publishers",
                    kWidth, kWidth);
    }

  private:
    static constexpr size_t kWidth = 6;
    std::array<rclcpp::Publisher<std_msgs::msg::Int32>::SharedPtr, kWidth> pubs_;
    std::array<rclcpp::Subscription<std_msgs::msg::Int32>::SharedPtr, kWidth> subs_;
    rclcpp::TimerBase::SharedPtr fast_;
    rclcpp::TimerBase::SharedPtr slow_;
    int ticks_ = 0;
};

} // namespace census_port_fanout

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    rclcpp::spin(std::make_shared<census_port_fanout::Fanout>());
    rclcpp::shutdown();
    return 0;
}
